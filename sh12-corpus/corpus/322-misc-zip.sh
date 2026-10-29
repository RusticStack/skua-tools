# M3-SH12 (zip group): zip + unzip interop. Git for Windows ships unzip but no zip, so when zip
# is missing the archives zip would write (names, sizes, methods, Info-ZIP's order: the directory
# entry, then its children by name) are embedded; the checks run on them with Git Bash's unzip.
# zip's own output lines ("  adding: x (deflated 93%)", "updating:", "deleting:") and exit codes
# are covered by the cargo tests of skua-tools (zip has no Git Bash reference to record).
mkdir -p proj/src proj/docs; cd proj
printf 'alpha\nbeta\n%.0s' {1..40} > a.txt; printf 'hello\n' > b.txt; printf '# Title\n' > docs/README.md
: > empty; printf 'fn main() {}\n' > src/main.rs
touch -d '2026-01-01 12:30:00' a.txt b.txt empty docs/README.md src/main.rs docs src .
cd ..; touch -d '2026-01-01 12:30:00' proj
if command -v zip >/dev/null 2>&1; then
  zip -qr full.zip proj; echo "zip -r rc=$?"
  zip -qj flat.zip proj/a.txt proj/src/main.rs; echo "zip -j rc=$?"
else
  echo "zip -r rc=0"; echo "zip -j rc=0"
  base64 -d > full.zip <<'EOF2'
UEsDBBQAAAAAAMBjIVwAAAAAAAAAAAAAAAAFAAAAcHJvai9QSwMEFAAAAAgAwGMhXH/XoKcTAAAA
uAEAAAoAAABwcm9qL2EudHh0S8wpyEjkSkotSeRKHGUmDRkmAFBLAwQUAAAAAADAYyFcIDA6NgYA
AAAGAAAACgAAAHByb2ovYi50eHRoZWxsbwpQSwMEFAAAAAAAwGMhXAAAAAAAAAAAAAAAAAoAAABw
cm9qL2RvY3MvUEsDBBQAAAAAAMBjIVzur5SDCAAAAAgAAAATAAAAcHJvai9kb2NzL1JFQURNRS5t
ZCMgVGl0bGUKUEsDBBQAAAAAAMBjIVwAAAAAAAAAAAAAAAAKAAAAcHJvai9lbXB0eVBLAwQUAAAA
AADAYyFcAAAAAAAAAAAAAAAACQAAAHByb2ovc3JjL1BLAwQUAAAAAADAYyFcy+lNOA0AAAANAAAA
EAAAAHByb2ovc3JjL21haW4ucnNmbiBtYWluKCkge30KUEsBAhQDFAAAAAAAwGMhXAAAAAAAAAAA
AAAAAAUAAAAAAAAAAAAQAO1BAAAAAHByb2ovUEsBAhQDFAAAAAgAwGMhXH/XoKcTAAAAuAEAAAoA
AAAAAAAAAAAAAKSBIwAAAHByb2ovYS50eHRQSwECFAMUAAAAAADAYyFcIDA6NgYAAAAGAAAACgAA
AAAAAAAAAAAApIFeAAAAcHJvai9iLnR4dFBLAQIUAxQAAAAAAMBjIVwAAAAAAAAAAAAAAAAKAAAA
AAAAAAAAEADtQYwAAABwcm9qL2RvY3MvUEsBAhQDFAAAAAAAwGMhXO6vlIMIAAAACAAAABMAAAAA
AAAAAAAAAKSBtAAAAHByb2ovZG9jcy9SRUFETUUubWRQSwECFAMUAAAAAADAYyFcAAAAAAAAAAAA
AAAACgAAAAAAAAAAAAAApIHtAAAAcHJvai9lbXB0eVBLAQIUAxQAAAAAAMBjIVwAAAAAAAAAAAAA
AAAJAAAAAAAAAAAAEADtQRUBAABwcm9qL3NyYy9QSwECFAMUAAAAAADAYyFcy+lNOA0AAAANAAAA
EAAAAAAAAAAAAAAApIE8AQAAcHJvai9zcmMvbWFpbi5yc1BLBQYAAAAACAAIAMkBAAB3AQAAAAA=
EOF2
  base64 -d > flat.zip <<'EOF2'
UEsDBBQAAAAIAMBjIVx/16CnEwAAALgBAAAFAAAAYS50eHRLzCnISORKSi1J5EocZSYNGSYAUEsD
BBQAAAAAAMBjIVzL6U04DQAAAA0AAAAHAAAAbWFpbi5yc2ZuIG1haW4oKSB7fQpQSwECFAMUAAAA
CADAYyFcf9egpxMAAAC4AQAABQAAAAAAAAAAAAAApIEAAAAAYS50eHRQSwECFAMUAAAAAADAYyFc
y+lNOA0AAAANAAAABwAAAAAAAAAAAAAApIE2AAAAbWFpbi5yc1BLBQYAAAAAAgACAGgAAABoAAAA
AAA=
EOF2
fi
unzip -l full.zip
unzip -Z1 full.zip
unzip -l flat.zip
unzip -t full.zip | tail -1
unzip -tq flat.zip
mkdir x; unzip -q full.zip -d x; echo "unzip rc=$?"; find x | sort
for f in a.txt b.txt empty docs/README.md src/main.rs; do cmp "proj/$f" "x/proj/$f" && echo "same $f"; done
mkdir y; unzip -q flat.zip -d y; cmp proj/a.txt y/a.txt && cmp proj/src/main.rs y/main.rs && echo flat-same
unzip -p full.zip proj/b.txt proj/src/main.rs
