#!/usr/bin/env bash
# Drive the real quai-kde QML (tests/ui-test.qml) against the quai and quaid of
# the current environment; tests/e2e/run.sh calls it with QUAI_E2E_KDE set.
# Fails on any failed step or QML runtime error.
#
#   KIRIGAMI=stub    (default) build and use tests/kirigami-stub: checks the
#                    page logic and bindings without KDE Frameworks installed
#   KIRIGAMI=system  use the installed Kirigami and Breeze style (kf6-kirigami,
#                    kf6-qqc2-desktop-style): checks the real component APIs
#
# The language follows LANG/LANGUAGE; with QUAI_LOCALEDIR pointing at built
# catalogues (make translations), a French run also checks the translation.
#
# Usage: tests/kde-ui-test.sh [path/to/quai-kde]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="${1:-$here/../../../target/debug/quai-kde}"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

case "${KIRIGAMI:-stub}" in
  stub)
    build="${KIRIGAMI_STUB_BUILD:-/tmp/kirigami-stub-build}"
    cmake -S "$here/kirigami-stub" -B "$build" -G Ninja -DCMAKE_BUILD_TYPE=Release >/dev/null
    cmake --build "$build" >/dev/null
    export QML_IMPORT_PATH="$build/qml" QT_QUICK_CONTROLS_STYLE=Basic
    ;;
  system) ;; # quai-kde selects org.kde.desktop itself
  *) echo "KIRIGAMI must be stub or system" >&2; exit 2 ;;
esac

# Fedora's Qt turns debug messages off (qtlogging.ini); console.log is one of them.
export QT_LOGGING_RULES="${QT_LOGGING_RULES:+$QT_LOGGING_RULES;}qml.debug=true;js.debug=true"
QT_QPA_PLATFORM=offscreen QUAI_KDE_MAIN="$here/ui-test.qml" timeout 120 "$bin" >"$log" 2>&1 || true
grep -E "^qml: (ok|UI TEST)" "$log" | sed 's/^qml: /    /' || true
errors='TypeError|ReferenceError|is not defined|Cannot assign|Unable to assign|Binding loop|non-existent|is not a type|unavailable|Cannot read property|module .* is not installed'
if grep -qE "$errors" "$log"; then
  echo "QML runtime errors:"
  grep -E "$errors" "$log" | sort -u | head -20
  exit 1
fi
grep -q "UI TEST PASSED" "$log" || { tail -20 "$log"; exit 1; }
