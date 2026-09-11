#!/bin/sh
# Installs a package in a clean container of its own distribution, and runs
# what it installed.
#
#   packaging/check-package.sh <package file> <version>
#
# From the repository root, e.g.
#
#   docker run --rm -v "$PWD:/src" -w /src debian:12 \
#     sh packaging/check-package.sh out/ctail_1.0.0_amd64.deb 1.0.0
#
# Weak dependencies are deliberately *not* installed: this proves the package
# installs, and the program starts and draws, with nothing but what the package
# insists on. Every weak dependency it names is then looked up in the
# distribution's repositories instead, because apt and dnf both skip a
# Recommends they cannot find without a word, and a misspelt one would ship
# unnoticed. (apk and pacman have no weak dependencies; for them the install
# itself resolves every name.)
set -eu

package=$1
version=$2
missing=""

case "$package" in
  *.deb)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends "./$package" > /dev/null
    for name in $(dpkg-deb -f "$package" Recommends Suggests \
      | sed -e 's/^[A-Za-z-]*: //' -e 's/([^)]*)//g' -e 's/[,|]/ /g'); do
      apt-cache show "$name" > /dev/null 2>&1 || missing="$missing $name"
    done
    ;;
  *.rpm)
    dnf install -y -q --setopt=install_weak_deps=False "./$package" > /dev/null
    for capability in $(rpm -qp --recommends "$package") $(rpm -qp --suggests "$package"); do
      [ -n "$(dnf -q repoquery --whatprovides "$capability" 2> /dev/null)" ] \
        || missing="$missing $capability"
    done
    ;;
  *.apk)
    apk add --no-cache --allow-untrusted "./$package" > /dev/null
    ;;
  *.pkg.tar.zst)
    # pacman 7 downloads as an unprivileged user behind a seccomp filter, and
    # a container's own seccomp profile can refuse to let it install one
    # ("error restricting syscalls via seccomp"). In a container thrown away
    # after this, the sandbox protects nothing.
    pacman -Sy --noconfirm --disable-sandbox > /dev/null
    pacman -U --noconfirm --disable-sandbox "./$package" > /dev/null
    ;;
  *)
    echo "::error::do not know how to install $package"
    exit 1
    ;;
esac

if [ -n "$missing" ]; then
  echo "::error::$package recommends or suggests what this distribution does not have:$missing"
  exit 1
fi

# Not /usr/share/doc: the Ubuntu container image tells dpkg to skip it, so its
# absence there says nothing about the package.
for file in /usr/bin/ctail /usr/share/applications/ctail.desktop \
  /usr/share/icons/hicolor/48x48/apps/ctail.png; do
  test -f "$file" || {
    echo "::error::$package did not install $file"
    exit 1
  }
done

# shellcheck source=/dev/null
echo "installed $package on $(. /etc/os-release && echo "$PRETTY_NAME") ($(uname -m))"
sh packaging/smoke.sh /usr/bin/ctail "$version"
