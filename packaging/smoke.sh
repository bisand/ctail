#!/bin/sh
# The program starts, says the version being released, and draws.
#
#   packaging/smoke.sh <program> <version>
#
# Run by the release workflow on every binary before it is packaged, and by
# check-package.sh on the installed program inside a clean container of each
# distribution. POSIX sh on purpose: it has to run under Alpine's busybox and
# Git Bash on Windows as well as bash.
#
# `--snapshot main` paints the main window, with a log open in it, into a PPM
# through the software rasteriser. It needs no display, no GPU and none of the
# window system's libraries, which is what lets it run on a headless runner and
# in a bare container — and it still goes through the engine, the fonts, the
# theme and every widget the window is made of.
set -eu

program=$1
version=$2

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# A native Windows program does not understand Git Bash's /tmp/... paths, and
# environment variables are handed over unconverted, so name the directory the
# way Windows does. Forward slashes, which Windows accepts and Rust keeps.
native=$work
if command -v cygpath > /dev/null 2>&1; then
  native=$(cygpath -m "$work")
fi

said=$("$program" --version || true)
echo "$program --version: $said"
if [ "$said" != "ctail $version" ]; then
  case "$program" in
    # A release build for Windows is a GUI-subsystem program, which has no
    # console of its own. Handed a pipe it writes to it, which is what $(...)
    # gives it here — but if a runner ever hands it nothing, an empty answer is
    # not a wrong one, and the workflow's version guard has already compared
    # the tag with desktop/Cargo.toml, which is where this string comes from.
    # A *wrong* answer still fails.
    *.exe)
      if [ -z "$said" ]; then
        echo "::notice::$program printed nothing for --version (a GUI-subsystem program); relying on the Cargo.toml version guard."
      else
        echo "::error::$program says '$said'; the version being built is $version"
        exit 1
      fi
      ;;
    *)
      echo "::error::$program says '$said'; the version being built is $version"
      exit 1
      ;;
  esac
fi

cat > "$work/sample.log" << 'LOG'
2026-09-11T08:00:00.000Z INFO  ctail smoke test starting
2026-09-11T08:00:00.120Z DEBUG reading configuration from the environment
2026-09-11T08:00:01.004Z WARN  disk usage at 91% on /var
2026-09-11T08:00:02.310Z ERROR connection refused: upstream 10.0.0.7:5432
2026-09-11T08:00:02.311Z INFO  retrying in 5 seconds
LOG

CTAIL_CONFIG_DIR="$native/config" CTAIL_DEBUG_FILE="$native/sample.log" \
  "$program" --snapshot main "$native/frame.ppm" 1

test -s "$work/frame.ppm"
magic=$(head -c 2 "$work/frame.ppm")
if [ "$magic" != "P6" ]; then
  echo "::error::$program --snapshot wrote something that is not a PPM"
  exit 1
fi
echo "$program drew a frame of $(wc -c < "$work/frame.ppm") bytes"
