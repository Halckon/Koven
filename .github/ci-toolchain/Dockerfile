# Official Ubuntu 26.04 linux/amd64 manifest, verified 2026-11-06.
FROM ubuntu:26.04@sha256:157dc3d6e4a3e4f2c68d12df9df9ed6e808b59af6d4c3757b432b4f519916725

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LLVM_SYS_211_PREFIX=/usr/lib/llvm-21 \
    PATH=/opt/rust/bin:/usr/lib/llvm-21/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Signed Ubuntu repositories remain mandatory. Final transitive package identities
# are recorded below and frozen by the published image digest, not by mutable tags.
RUN apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=30 \
      -o APT::Update::Error-Mode=any update \
    && apt-get install --yes --no-install-recommends \
      ca-certificates curl git python3 sudo xz-utils build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Build context contains only these public toolchain inputs, never the checkout,
# Cargo registry, credentials, test outputs, or a developer's HOME.
COPY install_ci_llvm_ubuntu26.sh install_ci_image_rust.sh verify_ci_toolchain_image.sh /opt/koven-ci/
RUN bash /opt/koven-ci/install_ci_llvm_ubuntu26.sh \
    && bash /opt/koven-ci/install_ci_image_rust.sh \
    && bash /opt/koven-ci/verify_ci_toolchain_image.sh \
    && dpkg-query -W -f='${binary:Package} ${Version} ${Architecture}\n' \
         > /opt/koven-ci/installed-packages.txt \
    && sha256sum /opt/koven-ci/*.sh > /opt/koven-ci/input-sha256.txt \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*.deb

# Cargo tools live outside HOME, which Actions replaces with its own mount.
# Jobs choose their own writable CARGO_HOME/target; no crate cache enters the image.
WORKDIR /workspace
CMD ["bash"]
