# Kirigami test stub

**Test-only.** A minimal stand-in for `org.kde.kirigami` (KF6) exposing just the
API quai-kde uses, so `tests/kde-ui-test.sh` can execute the real QML pages and
catch JavaScript and binding errors where Kirigami 6 is not installable (CI
images with an old Qt). It does not look like Kirigami and is never installed.
