"""Inspect real host loading dependencies for the bounded preview contract."""
import base64
import hashlib
import os
from pathlib import Path
import platform
import re
import selectors
import subprocess
import time
from urllib.parse import urlsplit


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


def _redact_apt_sources(text):
    """Keep only public Ubuntu archive URLs; never persist other source URIs."""
    redacted = False

    def replace(match):
        nonlocal redacted
        value = match.group(0)
        try:
            uri = urlsplit(value)
            if (uri.scheme in {'http', 'https'}
                    and uri.netloc in {'archive.ubuntu.com', 'azure.archive.ubuntu.com',
                                       'security.ubuntu.com'}
                    and uri.path in {'/ubuntu', '/ubuntu/'}
                    and '?' not in value and '#' not in value):
                return value
        except ValueError:
            pass
        redacted = True
        return '<redacted-source>'

    # cdrom labels can contain spaces; drop the rest of that source line too.
    # Enumerate non-hierarchical APT schemes, never package names like libxml2:amd64.
    pattern = (r'\b(?:cdrom:[^\r\n]*|[A-Za-z][A-Za-z0-9+.-]*:/{1,2}\S+'
               r'|(?:mirror|file|copy|rsh|ssh|ftp|http|https|tor):\S*)')
    return re.sub(pattern, replace, text, flags=re.IGNORECASE), redacted


def _diagnostic_probe(args, records, timeout=60, output_limit=32 * 1024 * 1024):
    """Bound both pipes while reading and reap every diagnostic subprocess."""
    argv = [str(arg) for arg in args]
    record = {'argv': argv, 'exit': None, 'timed_out': False, 'output_limited': False,
              'success': False, 'redacted': False, 'timeout_seconds': timeout,
              'output_limit_bytes_per_stream': output_limit, 'locale': 'C'}
    started = time.monotonic()
    buffers = {'stdout': bytearray(), 'stderr': bytearray()}
    process = None
    try:
        process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env={**os.environ, 'LC_ALL': 'C'})
        with selectors.DefaultSelector() as selected:
            selected.register(process.stdout, selectors.EVENT_READ, 'stdout')
            selected.register(process.stderr, selectors.EVENT_READ, 'stderr')
            while selected.get_map():
                remaining = timeout - (time.monotonic() - started)
                if remaining <= 0:
                    record['timed_out'] = True
                    break
                limited = False
                for key, _ in selected.select(remaining):
                    target = buffers[key.data]
                    capacity = output_limit - len(target)
                    chunk = os.read(key.fd, min(65536, capacity + 1))
                    if not chunk:
                        selected.unregister(key.fileobj)
                        continue
                    target.extend(chunk[:capacity])
                    if len(chunk) > capacity:
                        record['output_limited'] = limited = True
                        break
                if limited:
                    break
        if not record['timed_out'] and not record['output_limited']:
            try:
                process.wait(timeout=max(0, timeout - (time.monotonic() - started)))
            except subprocess.TimeoutExpired:
                record['timed_out'] = True
    except Exception as error:
        # Exception messages may contain unredacted command output or source URIs.
        record['error'] = type(error).__name__
    finally:
        if process is not None:
            try:
                if process.poll() is None:
                    process.kill()
                record['exit'] = process.wait()
            except Exception as error:
                record['error'] = type(error).__name__
            for stream in (process.stdout, process.stderr):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as error:
                        record['error'] = type(error).__name__
        output = {name: bytes(data) for name, data in buffers.items()}
        if argv and argv[0] == '/usr/bin/apt-cache':
            try:
                for name, data in output.items():
                    safe, redacted = _redact_apt_sources(data.decode('utf-8', errors='strict'))
                    output[name] = safe.encode('utf-8')
                    record['redacted'] |= redacted
            except Exception:
                output = {'stdout': b'', 'stderr': b''}
                record['error'] = 'RedactionError'
        record.update(elapsed_seconds=time.monotonic() - started,
                      stdout_base64=base64.b64encode(output['stdout']).decode('ascii'),
                      stderr_base64=base64.b64encode(output['stderr']).decode('ascii'))
        records.append(record)
    if (record['exit'] == 0 and not record['timed_out']
            and not record['output_limited'] and 'error' not in record):
        try:
            text = output['stdout'].decode('utf-8', errors='strict')
        except UnicodeError:
            record['error'] = 'UnicodeError'
        else:
            record['success'] = True
            return text
    return None


def collect_libxml2_diagnostic(binary, libraries, evidence):
    """CI-only read-only evidence, separate from every existing host verdict."""
    diagnostic = {'schema': 'koven.preview.libxml2-diagnostic.v1', 'complete': False,
                  'libraries': [], 'requesters': [], 'commands': [], 'errors': []}
    evidence['libxml2_diagnostic'] = diagnostic
    try:
        rows = [row for row in libraries if row['name'] == 'libxml2.so.2']
        diagnostic['libraries'] = rows
        if len(rows) != 1:
            diagnostic['errors'].append('expected exactly one measured libxml2 library')
            return
        library = rows[0]

        def probe(args):
            return _diagnostic_probe(args, diagnostic['commands'])

        probe(['/usr/bin/dpkg-query', '-S', library['resolved']])
        probe(['/usr/bin/dpkg-query', '-W',
               '-f=${binary:Package}\t${Status}\t${Architecture}\t${Version}\t'
               '${source:Package}\t${source:Version}\n', 'libxml2:amd64'])
        probe(['/usr/bin/dpkg-query', '--control-show', 'libxml2:amd64', 'md5sums'])
        probe(['/usr/bin/apt-cache', 'policy', 'libxml2:amd64'])
        probe(['/usr/bin/apt-cache', 'policy'])
        targets = [str(binary), *(row['resolved'] for row in libraries)]
        for path in sorted(set(targets)):
            dynamic = probe(['/usr/bin/readelf', '--wide', '-d', path])
            if dynamic is None:
                continue
            if re.search(r'\(NEEDED\).*\[libxml2\.so\.2\]', dynamic):
                diagnostic['requesters'].append(path)
            elif path != library['resolved']:
                continue
            probe(['/usr/bin/readelf', '--wide', '-h', path])
            probe(['/usr/bin/readelf', '--wide', '--dyn-syms', path])
            probe(['/usr/bin/readelf', '--wide', '--version-info', path])
        if not diagnostic['requesters']:
            diagnostic['errors'].append('no measured direct libxml2 requester')
        diagnostic['complete'] = (not diagnostic['errors'] and bool(diagnostic['commands'])
                                  and all(row['success'] for row in diagnostic['commands']))
    except Exception as error:
        diagnostic['errors'].append(type(error).__name__)


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
        policy = _collect_libxml2_policy(binary, libraries, os_release, evidence)
        if policy is not None:
            host['libxml2_policy'] = policy
        if os.environ.get('KOVEN_PREVIEW_LIBXML2_EVIDENCE') == '1':
            collect_libxml2_diagnostic(binary, libraries, evidence)
    return host


LIBXML_POLICY = 'koven.preview.libxml2-ubuntu-noble.v1'
LIBXML_VERSIONS = {'2.9.14+dfsg-1.3ubuntu3.9', '2.9.14+dfsg-1.3ubuntu3.10'}


def _elf_dynamic(text):
    values = {}
    count = None
    rows = 0
    for line in text.splitlines():
        if not line.strip() or line.strip() == 'Tag        Type                         Name/Value':
            continue
        header = re.fullmatch(r'Dynamic section at offset 0x[0-9a-f]+ contains (\d+) entries:', line)
        if header:
            if count is not None:
                raise ValueError('duplicate ELF dynamic table')
            count = int(header[1])
            continue
        row = re.fullmatch(r'\s*0x[0-9a-f]+ \(([^)]+)\)\s+(.*)', line)
        if not row or count is None:
            raise ValueError('unrecognized ELF dynamic row')
        rows += 1
        tag, value = row.groups()
        if tag in {'NEEDED', 'SONAME', 'RPATH', 'RUNPATH'}:
            match = re.fullmatch(r'[^\[\]]*\[([^\[\]]*)\]', value)
            if not match:
                raise ValueError('invalid ELF dynamic string')
            values.setdefault(tag, []).append(match[1])
    if count is None or rows != count:
        raise ValueError('incomplete ELF dynamic table')
    return values


def _elf_header(text):
    fields = {}
    versions = []
    for line in text.splitlines():
        if line.strip() == 'ELF Header:' or not line.strip():
            continue
        key, separator, value = line.strip().partition(':')
        if not separator:
            raise ValueError('invalid ELF header')
        if key == 'Version':
            versions.append(value.strip())
            continue
        if key in fields:
            raise ValueError('invalid ELF header')
        fields[key] = value.strip()
    if versions != ['1 (current)', '0x1']:
        raise ValueError('unsupported ELF identification/header versions')
    if (fields.get('Class'), fields.get('Data'), fields.get('Machine')) != (
            'ELF64', "2's complement, little endian", 'Advanced Micro Devices X86-64'):
        raise ValueError('unsupported libxml ELF architecture')
    return {'class': 'ELF64', 'data': 'little', 'machine': 'EM_X86_64'}


def _elf_symbols(text):
    symbols = []
    count = None
    for line in text.splitlines():
        if not line.strip() or re.fullmatch(r'\s*Num:\s+Value\s+Size\s+Type\s+Bind\s+Vis\s+Ndx\s+Name', line):
            continue
        header = re.fullmatch(r"Symbol table '\.dynsym' contains (\d+) entries:", line)
        if header:
            if count is not None:
                raise ValueError('duplicate dynsym table')
            count = int(header[1])
            continue
        match = re.fullmatch(r'\s*(\d+):\s+([0-9a-f]+)\s+(0x[0-9a-f]+|\d+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)(?:\s+(.*))?', line)
        if not match or count is None:
            raise ValueError('unrecognized dynsym row')
        index, _, size, kind, binding, visibility, section, name = match.groups()
        if int(index) != len(symbols):
            raise ValueError('noncontiguous dynsym indices')
        name = name or ''
        version_index = None
        suffix = re.fullmatch(r'(.*) \((\d+)\)', name)
        if suffix:
            name, version_index = suffix[1], int(suffix[2])
        version, default = None, False
        if '@' in name:
            parts = re.fullmatch(r'([^@\s]+)(@@?)([^@\s]+)', name)
            if not parts:
                raise ValueError('invalid versioned symbol')
            name, marker, version = parts.groups()
            default = marker == '@@'
        elif version_index is not None:
            raise ValueError('orphan symbol version index')
        symbols.append({'symbol_index': int(index), 'name': name, 'version': version,
                        'version_index': version_index, 'type': kind, 'binding': binding,
                        'visibility': visibility, 'size': int(size, 16 if size.startswith('0x') else 10), 'section': section,
                        'default': default})
    if count is None or count != len(symbols):
        raise ValueError('incomplete dynsym table')
    return symbols


def _elf_versions(text, symbols):
    """Validate object-local version tables; repeated uses of an index are normal."""
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    no_info = 'No version information found in this file.'
    if lines == [no_info]:
        if any(row['version'] is not None or row['version_index'] is not None for row in symbols):
            raise ValueError('missing claimed ELF version tables')
        return {}, {}
    if not lines or no_info in lines:
        raise ValueError('empty or contradictory ELF version output')
    mode = None
    counts, actual = {}, {'symbols': 0, 'needs': 0, 'definitions': 0}
    indices, needs, definitions = [], {}, {}
    pending = None
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        header = re.fullmatch(r"Version (symbols|needs|definition) section '([^']+)' contains (\d+) entr(?:y|ies):", stripped)
        if header:
            if pending is not None and pending['remaining']:
                raise ValueError('incomplete ELF version block')
            pending = None
            mode = {'symbols': 'symbols', 'needs': 'needs', 'definition': 'definitions'}[header[1]]
            if mode in counts:
                raise ValueError('duplicate ELF version section')
            expected_section = {'symbols': '.gnu.version', 'needs': '.gnu.version_r',
                                'definitions': '.gnu.version_d'}[mode]
            if header[2] != expected_section:
                raise ValueError('unexpected ELF version section identity')
            counts[mode] = int(header[3])
            continue
        if re.fullmatch(r'Addr: 0x[0-9a-f]+\s+Offset: 0x[0-9a-f]+\s+Link: \d+ \([^)]*\)', stripped):
            if mode is None:
                raise ValueError('orphan version table address')
            continue
        if mode == 'symbols':
            row = re.fullmatch(r'([0-9a-f]+):\s+(.*)', stripped)
            if not row or int(row[1], 16) != len(indices):
                raise ValueError('invalid version symbol offset')
            rest = row[2]
            while rest.strip():
                entry = re.match(r'\s*([0-9a-f]+)(h?)\s*\(([^)]+)\)', rest)
                if not entry:
                    raise ValueError('unrecognized version symbol')
                indices.append((int(entry[1], 16), entry[2] == 'h', entry[3]))
                rest = rest[entry.end():]
            actual['symbols'] = len(indices)
        elif mode == 'needs':
            file = re.fullmatch(r'(?:0x)?[0-9a-f]+: Version: 1\s+File: (\S+)\s+Cnt: (\d+)', stripped)
            name = re.fullmatch(r'(?:0x)?[0-9a-f]+:\s+Name: (\S+)\s+Flags: (\S+)\s+Version: (\d+)', stripped)
            if file:
                if pending is not None and pending['remaining']:
                    raise ValueError('incomplete version need')
                pending = {'file': file[1], 'remaining': int(file[2])}
                actual['needs'] += 1
            elif name and pending and pending['remaining'] > 0:
                index = int(name[3])
                if index < 2 or index in needs or index in definitions:
                    raise ValueError('conflicting version definition')
                needs[index] = {'file': pending['file'], 'name': name[1], 'flags': name[2]}
                pending['remaining'] -= 1
            else:
                raise ValueError('unrecognized version need row')
        elif mode == 'definitions':
            definition = re.fullmatch(r'(?:0x)?[0-9a-f]+: Rev: 1\s+Flags: (\S+)\s+Index: (\d+)\s+Cnt: (\d+)\s+Name: (\S+)', stripped)
            parent = re.fullmatch(r'(?:0x)?[0-9a-f]+:\s+Parent \d+: (\S+)', stripped)
            if definition:
                if pending is not None and pending['remaining']:
                    raise ValueError('incomplete version definition')
                index = int(definition[2])
                if index < 1 or index in needs or index in definitions or int(definition[3]) < 1:
                    raise ValueError('conflicting version definition')
                definitions[index] = {'name': definition[4], 'flags': definition[1]}
                pending = {'remaining': int(definition[3]) - 1}
                actual['definitions'] += 1
            elif parent and pending and pending['remaining'] > 0:
                pending['remaining'] -= 1
            else:
                raise ValueError('unrecognized version definition row')
        else:
            raise ValueError('unrecognized ELF version output')
    if pending is not None and pending['remaining']:
        raise ValueError('incomplete final ELF version block')
    if any(actual[key] != value for key, value in counts.items()):
        raise ValueError('incomplete ELF version section')
    if indices and len(indices) != len(symbols):
        raise ValueError('dynsym/version symbol counts differ')
    if not indices and (needs or definitions or any(row['version'] for row in symbols)):
        raise ValueError('missing ELF version symbol table')
    for row, (index, hidden, label) in zip(symbols, indices):
        if index < 2:
            if row['version'] or label != ('*local*' if index == 0 else '*global*'):
                raise ValueError('invalid unversioned symbol index')
        else:
            mapping = needs.get(index) or definitions.get(index)
            if not mapping or mapping['name'] != label:
                raise ValueError('orphan ELF symbol version')
            if row['section'] == 'UND' and (index not in needs or row['version'] != label):
                raise ValueError('undefined symbol lacks explicit version need')
            if row['version'] and row['version'] != label:
                raise ValueError('symbol version text differs from index')
            if row['version_index'] is not None and row['version_index'] != index:
                raise ValueError('symbol version indices disagree')
        row['version_index'] = index
        row['version_hidden'] = hidden
    return needs, definitions


def _apt_environment(package_text, global_text):
    candidate = None
    sources = []
    selected = False
    for line in package_text.splitlines():
        match = re.fullmatch(r'\s*Candidate: (\S+)', line)
        if match:
            if candidate is not None:
                raise ValueError('duplicate APT candidate')
            candidate = match[1]
        version = re.fullmatch(r'\s*(?:\*\*\* )?(\S+)\s+\d+', line)
        if version:
            selected = version[1] == candidate
            continue
        if selected:
            source = re.fullmatch(r'\s*\d+\s+\S+\s+([^/\s]+)/([^/\s]+)\s+(\S+) Packages', line)
            if source:
                sources.append(dict(zip(('suite', 'component', 'architecture'), source.groups())))
            elif line.strip() and not re.fullmatch(r'\s*\d+ /var/lib/dpkg/status', line):
                raise ValueError('unrecognized candidate APT source')
    releases = []
    for line in global_text.splitlines():
        if line.strip().startswith('release '):
            fields = {}
            for field in line.strip()[8:].split(','):
                key, separator, value = field.partition('=')
                if not separator or key in fields:
                    raise ValueError('invalid APT release fields')
                fields[key] = value
            if fields.get('o') == 'Ubuntu' and fields.get('c') == 'main' and fields.get('b') == 'amd64':
                releases.append({name: fields.get(key) for key, name in
                                 [('a', 'suite'), ('c', 'component'), ('b', 'architecture'),
                                  ('o', 'origin'), ('l', 'label'), ('n', 'codename'), ('v', 'version')]})
    return {'candidate': candidate, 'package_sources': sources, 'releases': releases}


def _collect_libxml2_policy(binary, libraries, distribution, evidence):
    """Collect optional preflight facts; a failed collection never grants an exception."""
    diagnostic = {'commands': [], 'errors': []}
    evidence['libxml2_policy'] = diagnostic

    def probe(args):
        result = _diagnostic_probe(args, diagnostic['commands'])
        if result is None:
            raise ValueError('libxml policy probe failed')
        return result

    try:
        matches = [row for row in libraries if row['name'] == 'libxml2.so.2']
        if len(matches) != 1:
            raise ValueError('libxml library must be unique')
        library = matches[0]
        path = library['resolved']
        owner = probe(['/usr/bin/dpkg-query', '-S', path]).splitlines()
        if owner != [f'libxml2:amd64: {path}']:
            raise ValueError('libxml package ownership is not unique')
        package = probe(['/usr/bin/dpkg-query', '-W',
                         '-f=${binary:Package}\t${Status}\t${Architecture}\t${Version}\t'
                         '${source:Package}\t${source:Version}\n', 'libxml2:amd64']).splitlines()
        if len(package) != 1 or len(package[0].split('\t')) != 6:
            raise ValueError('invalid installed libxml package fields')
        package = dict(zip(('binary', 'status', 'architecture', 'version', 'source', 'source_version'),
                           package[0].split('\t')))
        relative = path.removeprefix('/')
        checksums = probe(['/usr/bin/dpkg-query', '--control-show', 'libxml2:amd64', 'md5sums'])
        md5 = []
        for line in checksums.splitlines():
            row = re.fullmatch(r'([0-9a-f]{32})  (.+)', line)
            if not row:
                raise ValueError('invalid package file checksum row')
            if row[2] == relative:
                md5.append(row[1])
        if len(md5) != 1:
            raise ValueError('libxml checksum path is not unique')
        content = Path(path).read_bytes()
        if hashlib.sha256(content).hexdigest() != library['sha256']:
            raise ValueError('libxml file changed during inspection')
        environment = _apt_environment(probe(['/usr/bin/apt-cache', 'policy', 'libxml2:amd64']),
                                       probe(['/usr/bin/apt-cache', 'policy']))
        records = {}
        for target in sorted({str(binary), *(row['resolved'] for row in libraries)}):
            dynamic = _elf_dynamic(probe(['/usr/bin/readelf', '--wide', '-d', target]))
            symbols = _elf_symbols(probe(['/usr/bin/readelf', '--wide', '--dyn-syms', target]))
            needs, definitions = _elf_versions(
                probe(['/usr/bin/readelf', '--wide', '--version-info', target]), symbols)
            records[target] = (dynamic, symbols, needs, definitions)
        provider_dynamic, provider_symbols, _, provider_versions = records[path]
        elf = _elf_header(probe(['/usr/bin/readelf', '--wide', '-h', path]))
        sonames = provider_dynamic.get('SONAME', [])
        elf.update(soname=sonames[0] if len(sonames) == 1 else None,
                   needed=sorted(provider_dynamic.get('NEEDED', [])),
                   rpaths=provider_dynamic.get('RPATH', []), runpaths=provider_dynamic.get('RUNPATH', []))
        requesters, unversioned = [], set()
        for target, (dynamic, symbols, needs, _) in records.items():
            imports = [row for row in symbols if row['section'] == 'UND' and row['name']]
            unversioned.update(row['name'] for row in imports if row['version'] is None)
            xml_needs = {index: value for index, value in needs.items() if value['file'] == 'libxml2.so.2'}
            if any(row['section'] != 'UND' and row['version_index'] in xml_needs for row in symbols):
                raise ValueError('defined symbol uses unsupported libxml version need')
            if 'libxml2.so.2' not in dynamic.get('NEEDED', []):
                if xml_needs:
                    raise ValueError('libxml version need outside direct requester')
                continue
            _elf_header(probe(['/usr/bin/readelf', '--wide', '-h', target]))
            requirements = []
            for row in imports:
                if row['version_index'] not in xml_needs:
                    continue
                need = xml_needs[row['version_index']]
                if need['flags'] != 'none' or row.get('version_hidden') or row['version'] != need['name']:
                    raise ValueError('unsupported libxml version requirement')
                requirements.append({key: row[key] for key in
                                     ('symbol_index', 'name', 'version_index', 'version', 'type',
                                      'binding', 'visibility', 'size') } | {'file': need['file']})
            requesters.append({'path': target, 'sha256': digest(target),
                               'requirements': sorted(requirements, key=lambda row: (row['name'], row['version']))})
        required = {(row['name'], row['version']) for requester in requesters for row in requester['requirements']}
        exports = [row for row in provider_symbols if row['section'] != 'UND' and row['name']]
        definitions = []
        for row in exports:
            if (row['name'], row['version']) not in required:
                continue
            if not row['section'].isdigit() or int(row['section']) <= 0:
                raise ValueError('required libxml definition has unsupported section')
            version = provider_versions.get(row['version_index'])
            if (not version or version['name'] != row['version'] or version['flags'] != 'none'
                    or row.get('version_hidden')):
                raise ValueError('invalid required libxml version definition')
            definitions.append({key: row[key] for key in
                                ('symbol_index', 'name', 'version_index', 'version', 'type',
                                 'binding', 'visibility', 'size', 'default')})
        policy = {'policy': LIBXML_POLICY, 'library': dict(library),
                  'distribution': {'id': distribution.get('ID'), 'version': distribution.get('VERSION_ID'),
                                   'codename': distribution.get('VERSION_CODENAME')},
                  'package': package, 'file_md5': {'path': relative, 'expected': md5[0],
                                                   'actual': hashlib.md5(content, usedforsecurity=False).hexdigest()},
                  'apt_environment': environment, 'installed_origin': 'unproven',
                  'source_uri_status': 'unproven', 'elf': elf, 'requesters': requesters,
                  'definitions': sorted(definitions, key=lambda row: (row['name'], row['version'])),
                  'unversioned_imports': sorted(unversioned),
                  'exported_names': sorted({row['name'] for row in exports})}
        if digest(path) != library['sha256']:
            raise ValueError('libxml file changed after ELF inspection')
        return policy
    except (ValueError, OSError, KeyError, TypeError) as error:
        diagnostic['errors'].append(str(error))
        return None


def _valid_libxml_policy(host, library):
    """Validate each side independently; matching invalid claims are not enough."""
    policy = host.get('libxml2_policy')
    if not isinstance(policy, dict) or policy.get('policy') != LIBXML_POLICY:
        return False
    if (host.get('system'), host.get('arch'), host.get('os_version')) != ('Linux', 'x86_64', '24.04'):
        return False
    if policy.get('library') != library or policy.get('distribution') != {
            'id': 'ubuntu', 'version': '24.04', 'codename': 'noble'}:
        return False
    package = policy['package']
    if (package.get('binary'), package.get('status'), package.get('architecture'), package.get('source')) != (
            'libxml2:amd64', 'install ok installed', 'amd64', 'libxml2'):
        return False
    if package.get('version') not in LIBXML_VERSIONS or package.get('source_version') != package['version']:
        return False
    md5 = policy['file_md5']
    if (md5.get('path') != library['resolved'].removeprefix('/')
            or not isinstance(md5.get('expected'), str) or not re.fullmatch('[0-9a-f]{32}', md5['expected'])
            or md5.get('actual') != md5['expected']):
        return False
    environment = policy['apt_environment']
    if environment.get('candidate') not in LIBXML_VERSIONS:
        return False
    sources, releases = environment['package_sources'], environment['releases']
    if not sources or not releases:
        return False
    allowed_suites = {'noble', 'noble-updates', 'noble-security'}
    for source in sources:
        if (source.get('suite') not in allowed_suites or source.get('component') != 'main'
                or source.get('architecture') != 'amd64'):
            return False
        if not any(release == {**source, 'origin': 'Ubuntu', 'label': 'Ubuntu',
                               'codename': 'noble', 'version': '24.04'} for release in releases):
            return False
    if policy.get('installed_origin') != 'unproven' or policy.get('source_uri_status') != 'unproven':
        return False
    elf = policy['elf']
    if any(elf.get(key) != value for key, value in {'class': 'ELF64', 'data': 'little',
           'machine': 'EM_X86_64', 'soname': 'libxml2.so.2', 'rpaths': [], 'runpaths': []}.items()):
        return False
    needed = elf['needed']
    if not isinstance(needed, list) or not needed or len(needed) != len(set(needed)):
        return False
    if not set(needed) <= {row['name'] for row in host['libraries']}:
        return False
    requesters = policy['requesters']
    llvm = [row for row in host['libraries'] if row['name'] == 'libLLVM.so.21.1']
    if len(llvm) != 1 or len(requesters) != 1:
        return False
    requester = requesters[0]
    if requester.get('path') != llvm[0]['resolved'] or requester.get('sha256') != llvm[0]['sha256']:
        return False
    requirements = requester['requirements']
    definitions = policy['definitions']
    if not requirements or len(requirements) != len(definitions):
        return False
    keys = [(row['name'], row['version']) for row in requirements]
    if len(keys) != len(set(keys)):
        return False
    defined = {(row['name'], row['version']): row for row in definitions}
    if len(defined) != len(definitions) or set(keys) != set(defined):
        return False
    for row in requirements:
        definition = defined[(row['name'], row['version'])]
        if (row.get('file') != 'libxml2.so.2' or row.get('type') not in {'FUNC', 'OBJECT'}
                or row.get('binding') != 'GLOBAL' or row.get('visibility') != 'DEFAULT'):
            return False
        for symbol in (row, definition):
            if (not isinstance(symbol.get('name'), str) or not symbol['name']
                    or not isinstance(symbol.get('version'), str) or not symbol['version']
                    or any(type(symbol.get(key)) is not int or symbol[key] < minimum
                           for key, minimum in [('size', 0), ('symbol_index', 0), ('version_index', 2)])):
                return False
        if (definition.get('type') != row['type'] or definition.get('binding') != 'GLOBAL'
                or definition.get('visibility') != 'DEFAULT' or definition.get('default') is not True):
            return False
        if row['type'] == 'OBJECT' and (definition['size'] <= 0 or row['size'] not in (0, definition['size'])):
            return False
    for symbols in (requirements, definitions):
        if len({row['symbol_index'] for row in symbols}) != len(symbols):
            return False
        version_names = {}
        for row in symbols:
            previous = version_names.setdefault(row['version_index'], row['version'])
            if previous != row['version']:
                return False
    for field in ('unversioned_imports', 'exported_names'):
        value = policy[field]
        if not isinstance(value, list) or any(not isinstance(name, str) or not name for name in value):
            return False
        if value != sorted(set(value)):
            return False
    if not {row['name'] for row in definitions} <= set(policy['exported_names']):
        return False
    return True


def _libxml_difference_allowed(producer, consumer):
    try:
        rows = []
        for host in (producer, consumer):
            libraries = host['libraries']
            if not isinstance(libraries, list) or any(not isinstance(row, dict) for row in libraries):
                return False
            if any(not isinstance(row.get('name'), str) or not row['name']
                   or not isinstance(row.get('resolved'), str) or not row['resolved'].startswith('/')
                   or not isinstance(row.get('sha256'), str) or not re.fullmatch('[0-9a-f]{64}', row['sha256'])
                   for row in libraries):
                return False
            keys = [(row['name'], row['resolved']) for row in libraries]
            if len(keys) != len(set(keys)) or len({row['name'] for row in libraries}) != len(keys):
                return False
            rows.append({row['name']: row for row in libraries})
        left, right = rows
        if set(left) != set(right) or 'libxml2.so.2' not in left:
            return False
        for name in left:
            if left[name]['resolved'] != right[name]['resolved']:
                return False
            if name != 'libxml2.so.2' and left[name] != right[name]:
                return False
        if not all(_valid_libxml_policy(host, row['libxml2.so.2'])
                   for host, row in zip((producer, consumer), rows)):
            return False
        p, c = producer['libxml2_policy'], consumer['libxml2_policy']
        if p['elf'] != c['elf'] or p['requesters'] != c['requesters']:
            return False
        p_defs = {(row['name'], row['version']): row for row in p['definitions']}
        for row in c['definitions']:
            if row['type'] == 'OBJECT' and row['size'] != p_defs[(row['name'], row['version'])]['size']:
                return False
        exports = set(p['exported_names']) | set(c['exported_names'])
        return not exports.intersection(set(p['unversioned_imports']) | set(c['unversioned_imports']))
    except (KeyError, TypeError, ValueError, AttributeError):
        return False


def compare_hosts(producer, consumer):
    """Match the actual closure, rather than trusting an installed package name."""
    errors = []
    for key in ('system', 'arch', 'llvm_version', 'llvm_prefix', 'c_driver', 'sdk', 'sdk_path', 'glibc'):
        if producer.get(key) != consumer.get(key):
            errors.append(f'host {key} differs from the candidate')
    expected = [(row['name'], row.get('resolved'), row['sha256']) for row in producer.get('libraries', [])]
    actual = [(row['name'], row.get('resolved'), row['sha256']) for row in consumer.get('libraries', [])]
    if not expected or sorted(expected) != sorted(actual):
        if not _libxml_difference_allowed(producer, consumer):
            errors.append('actual loading libraries differ from the candidate')
    if producer.get('system') == 'Darwin':
        if version_tuple(consumer['os_version']) < version_tuple(producer['minimum_os']):
            errors.append('consumer macOS is below the actual loading closure minimum')
    elif producer.get('os_version') != consumer.get('os_version'):
        errors.append('consumer OS differs from the measured Linux environment')
    return errors
