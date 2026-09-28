# Ubuntu image with bosun and hats installed and ready to use.
#
#   docker build -t bosun .
#   docker run --rm -it bosun
#
# install.sh runs at build time with CI=1, so Homebrew, bosun and hats are
# baked into the image and the container starts usable. Nothing is configured:
# the init wizards are interactive and belong to whoever runs the container,
# not to the build.
#
# This is not .devcontainer/Dockerfile. That one builds bosun from the mounted
# source to test it on Linux; this one installs the released formulae from the
# tap, which is what a user gets.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Homebrew's own prerequisites, plus zsh because that is the shell bosun targets.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential procps curl file git ca-certificates \
      zsh sudo locales tzdata less \
    && locale-gen en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8

# Homebrew refuses to run as root, so the image needs an ordinary user. sudo is
# passwordless because the Homebrew installer asks for it during setup.
ARG USER=dev
RUN useradd -m -s /usr/bin/zsh ${USER} \
    && echo "${USER} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/${USER} \
    && chmod 0440 /etc/sudoers.d/${USER}

USER ${USER}
WORKDIR /home/${USER}

# Copied rather than curled so the image builds from this checkout, and a change
# to the script invalidates the layer below it.
COPY --chown=${USER}:${USER} install.sh /tmp/install.sh

# CI=1 answers the Homebrew prompt. Without it the build would hang waiting for
# a terminal that a docker build does not have.
RUN CI=1 sh /tmp/install.sh && rm /tmp/install.sh

# brew shellenv is what install.sh evaluated for itself; bake the same PATH in
# so every later layer, and every shell in the container, finds brew, bosun and hats.
ENV PATH="/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:/home/${USER}/.local/bin:${PATH}"

# Interactive shells get brew's environment properly (MANPATH, INFOPATH too).
RUN echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> /home/${USER}/.zshrc

RUN bosun --version && bosun doctor || true

CMD ["zsh"]
