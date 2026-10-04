"""Inspect real host loading dependencies for the bounded preview contract."""
import base64
import hashlib
import os
from pathlib import Path
import platform
import re
import subprocess
import time


class HostInspectionError(ValueError):
    """Carry original commands and partial host facts across the package boundary."""
    def __init__(self, error, evidence):
        super().__init__(str(error))
        self.evidence = evidence


def command(args, evidence=None):
    """Fail rather than infer a dependency when a host tool cannot inspect it."""
    result = run_probe(args, os.environ, evidence)
    if result.returncode != 0:
        raise ValueError(f'host inspection command failed with exit {result.returncode}')
    return result.stdout.decode('utf-8', errors='strict').strip()


def run_probe(args, environment, evidence):
    record = {'argv': [str(arg) for arg in args], 'exit': None, 'timed_out': False,
              'env': {key: value for key, value in environment.items()
                      if key.startswith(('DYLD_', 'LD_')) or key in ('LLVM_SYS_211_PREFIX', 'SDKROOT')}}
    started = time.monotonic()
    stdout, stderr = b'', b''
    try:
        result = subprocess.run(record['argv'], capture_output=True, timeout=60,
                                stdin=subprocess.DEVNULL, env=dict(environment))
        record['exit'] = result.returncode
        stdout, stderr = result.stdout, result.stderr
        return result
    except subprocess.TimeoutExpired as error:
        record['timed_out'] = True
        stdout, stderr = error.output or b'', error.stderr or b''
        raise
    finally:
        record.update(elapsed_seconds=time.monotonic() - started,
                      stdout_base64=base64.b64encode(stdout).decode('ascii'),
                      stderr_base64=base64.b64encode(stderr).decode('ascii'))
        if evidence is not None:
            evidence['commands'].append(record)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def version_tuple(value):
    parts = [int(part) for part in value.split('.')]
    while parts and parts[-1] == 0:
        parts.pop()
    return tuple(parts)


def allowed_path(path, roots):
    """Check canonical paths, including symlink targets and parent components."""
    real = Path(path).resolve()
    if not any(real.is_relative_to(Path(root).resolve()) for root in roots):
        raise ValueError('dependency path is outside the documented installation')
    return real


def check_loader_environment(environment):
    redirected = [key for key, value in environment.items()
                  if value and key.startswith(('DYLD_', 'LD_'))]
    if redirected:
        raise ValueError('loader overrides are not supported: ' + ', '.join(sorted(redirected)))


def verify_dyld_loads(text, binary, libraries):
    actual = set()
    for value in re.findall(r'^dyld\[\d+\]: <[^>]+> (.+)$', text, re.M):
        if Path(value).resolve() == Path(binary).resolve():
            continue
        if value.startswith(('/usr/lib/', '/System/', '/Library/Apple/System/Library/')):
            continue
        actual.add(str(allowed_path(value, ['/opt/homebrew'])))
    expected = {row['resolved'] for row in libraries}
    if not expected or actual != expected:
        raise ValueError('actual dyld loading closure differs from declared dependencies')
    return sorted(actual)


def macho_minimum(text):
    versions = []
    for block in re.split(r'^Load command \d+\n', text, flags=re.M):
        if re.search(r'^\s*cmd LC_BUILD_VERSION\s*$', block, re.M):
            key = 'minos'
        elif re.search(r'^\s*cmd LC_VERSION_MIN_MACOSX\s*$', block, re.M):
            key = 'version'
        else:
            continue
        match = re.search(rf'^\s*{key} (\d+(?:\.\d+)+)\s*$', block, re.M)
        if match:
            versions.append(match[1])
    if not versions:
        raise ValueError('Mach-O minimum OS could not be determined')
    return max(versions, key=version_tuple)


def parse_ldd(text):
    libraries = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('linux-vdso.'):
            continue
        if 'not found' in line:
            raise ValueError(f'unresolved ELF dependency: {line}')
        match = re.fullmatch(r'(?:(\S+) => )?(/\S+) \(0x[0-9a-fA-F]+\)', line)
        if not match:
            raise ValueError(f'unrecognized ELF dependency: {line}')
        name, path = match.groups()
        libraries[name or Path(path).name] = path
    if not libraries:
        raise ValueError('empty ELF loading closure')
    return dict(sorted(libraries.items()))


def mac_libraries(binary, evidence=None):
    """Traverse concrete external install names; leave system shared-cache files to OS."""
    pending = [Path(binary)]
    visited = set()
    external = {}
    records = []
    minimums = []
    while pending:
        path = pending.pop()
        real = path.resolve(strict=True)
        if real in visited:
            continue
        visited.add(real)
        loads = command(['/usr/bin/otool', '-L', path], evidence)
        headers = command(['/usr/bin/otool', '-l', path], evidence)
        minimums.append(macho_minimum(headers))
        rpaths = re.findall(r'cmd LC_RPATH\n\s*cmdsize \d+\n\s*path (.+) \(offset \d+\)', headers)
        for search in rpaths:
            resolved = search.replace('@loader_path', str(real.parent))
            if not resolved.startswith('/'):
                raise ValueError('unresolved Mach-O loader search path')
            allowed_path(resolved, ['/opt/homebrew'])
        records.append({'file': path.name, 'loads': loads.splitlines()[1:],
                        'minimum_os': minimums[-1], 'rpaths': rpaths})
        for line in loads.splitlines()[1:]:
            match = re.match(r'\s*(\S+) \(compatibility version', line)
            if not match:
                raise ValueError(f'unrecognized Mach-O dependency: {line}')
            name = match[1]
            if name.startswith(('/usr/lib/', '/System/Library/')):
                continue
            # This candidate deliberately supports external Homebrew LLVM, without relocation.
            if not name.startswith('/opt/homebrew/'):
                raise ValueError(f'undeclared Mach-O load path: {name}')
            dependency = allowed_path(name, ['/opt/homebrew'])
            if not dependency.is_file():
                raise ValueError('external Mach-O dependency is missing')
            if dependency == real:
                continue  # LC_ID_DYLIB is included by otool -L.
            if name not in external:
                external[name] = {'name': name, 'resolved': str(dependency),
                                  'sha256': digest(dependency)}
                pending.append(Path(name))
    if not any('libLLVM' in name for name in external):
        raise ValueError('candidate does not dynamically load external LLVM')
    return sorted(external.values(), key=lambda row: row['name']), records, max(minimums, key=version_tuple)


def inspect_host(binary, evidence=None):
    """Inspect the executable and all declared external libraries on this actual host."""
    evidence = {'commands': [], 'partial': {}} if evidence is None else evidence
    try:
        return _inspect_host(binary, evidence)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        raise HostInspectionError(error, evidence) from error


def _inspect_host(binary, evidence):
    binary = Path(binary).resolve(strict=True)
    check_loader_environment(os.environ)
    system = platform.system()
    arch = platform.machine()
    if (system, arch) not in {('Darwin', 'arm64'), ('Linux', 'x86_64')}:
        raise ValueError(f'unsupported preview host: {system}/{arch}')
    prefix = Path('/opt/homebrew/opt/llvm@21' if system == 'Darwin' else '/usr/lib/llvm-21')
    configured = os.environ.get('LLVM_SYS_211_PREFIX')
    if configured and Path(configured).resolve() != prefix.resolve():
        raise ValueError('LLVM prefix differs from the documented external installation')
    llvm_version = command([prefix / 'bin/llvm-config', '--version'], evidence)
    if not re.fullmatch(r'21\.1\.\d+', llvm_version):
        raise ValueError('expected actual LLVM 21.1.x installation')
    driver = '/usr/bin/clang' if system == 'Darwin' else '/usr/bin/cc'
    host = {'system': system, 'arch': arch, 'llvm_version': llvm_version,
            'llvm_prefix': str(prefix), 'c_driver': {'path': driver,
                                                 'version': command([driver, '--version'], evidence)}}
    evidence['partial'] = host
    if system == 'Darwin':
        libraries, records, minimum = mac_libraries(binary, evidence)
        host.update(libraries=libraries, load_records=records, minimum_os=minimum)
        probe = run_probe([binary, '--help'], {**os.environ, 'DYLD_PRINT_LIBRARIES': '1'}, evidence)
        if probe.returncode != 2 or b'unknown command --help' not in probe.stderr:
            raise ValueError('actual CLI loader probe did not reach the command boundary')
        loaded = verify_dyld_loads(probe.stderr.decode('utf-8', errors='strict'), binary, libraries)
        host.update(os_version=platform.mac_ver()[0], libraries=libraries,
                    load_records=records, minimum_os=minimum,
                    actual_loaded=loaded,
                    sdk=command(['/usr/bin/xcrun', '--show-sdk-version'], evidence),
                    sdk_path=command(['/usr/bin/xcrun', '--show-sdk-path'], evidence))
        if version_tuple(host['os_version']) < version_tuple(minimum):
            raise ValueError('external dependency requires a newer macOS than this host')
    else:
        os_release = platform.freedesktop_os_release()
        if (os_release.get('ID'), os_release.get('VERSION_ID')) != ('ubuntu', '24.04'):
            raise ValueError('Linux preview currently requires Ubuntu 24.04')
        dynamic = command(['/usr/bin/readelf', '-d', binary], evidence)
        headers = command(['/usr/bin/readelf', '-l', binary], evidence)
        if not re.search(r'\(NEEDED\).*\[libLLVM[^\]]*\]', dynamic):
            raise ValueError('candidate does not dynamically load external LLVM')
        for search in re.findall(r'\((?:RPATH|RUNPATH)\).*\[([^\]]*)\]', dynamic):
            for part in search.split(':'):
                if not part.startswith('/'):
                    raise ValueError('ELF has an unresolved loader search path')
                allowed_path(part, ['/usr/lib', '/lib'])
        libraries = []
        for name, value in parse_ldd(command(['/usr/bin/ldd', binary], evidence)).items():
            real = allowed_path(value, ['/usr/lib', '/lib'])
            if not real.is_file():
                raise ValueError('ELF dependency is missing')
            libraries.append({'name': name, 'resolved': str(real), 'sha256': digest(real)})
        host.update(os_version=os_release['VERSION_ID'], libraries=libraries,
                    load_records={'dynamic': dynamic, 'program_headers': headers},
                    glibc=command(['/usr/bin/getconf', 'GNU_LIBC_VERSION'], evidence))
    return host


def compare_hosts(producer, consumer):
    """Match the actual closure, rather than trusting an installed package name."""
    errors = []
    for key in ('system', 'arch', 'llvm_version', 'llvm_prefix', 'c_driver', 'sdk', 'sdk_path', 'glibc'):
        if producer.get(key) != consumer.get(key):
            errors.append(f'host {key} differs from the candidate')
    expected = [(row['name'], row.get('resolved'), row['sha256']) for row in producer.get('libraries', [])]
    actual = [(row['name'], row.get('resolved'), row['sha256']) for row in consumer.get('libraries', [])]
    if not expected or sorted(expected) != sorted(actual):
        errors.append('actual loading libraries differ from the candidate')
    if producer.get('system') == 'Darwin':
        if version_tuple(consumer['os_version']) < version_tuple(producer['minimum_os']):
            errors.append('consumer macOS is below the actual loading closure minimum')
    elif producer.get('os_version') != consumer.get('os_version'):
        errors.append('consumer OS differs from the measured Linux environment')
    return errors
