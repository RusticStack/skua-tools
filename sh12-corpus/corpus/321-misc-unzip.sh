# M3-SH12 (zip group): unzip as Git Bash's Info-ZIP UnZip 6.00 behaves for agents: the
# "Archive:" / "inflating:" / "creating:" / "extracting:" lines, -d -o -n -q -qq -j -p -l -t -Z1,
# member patterns (* stops at /), the table of -l, exit codes (1 warning, 9 no archive, 11 no
# match), the replace prompt on stderr when nothing can answer it (stdin is /dev/null), and
# zip-slip protection. The archives are embedded (DOS timestamps only, so no time zone).
mkdir z; cd z
base64 -d > a.zip <<'EOF2'
UEsDBBQAAAAAAMBjIVwAAAAAAAAAAAAAAAACAAAAZC9QSwMEFAAAAAgAwGMhXLoPAE0QAAAA8AAA
AAcAAABkL3gudHh0y0jNyclXyEAnuTBEhok4AFBLAwQUAAAAAADAYyFcAAAAAAAAAAAAAAAABAAA
AGQvcy9QSwMEFAAAAAAAwGMhXGyhSboDAAAAAwAAAAkAAABkL3MveS50eHR5eQpQSwMEFAAAAAAA
wGMhXCL0eEkEAAAABAAAAAcAAAB0b3AudHh0dG9wClBLAwQUAAAAAADAYyFcAAAAAAAAAAAAAAAA
BQAAAGUudHh0UEsDBBQAAAAAAMBjIVxOgYhHBAAAAAQAAAAKAAAAc3AgYWNlLnR4dGFiYwpQSwME
FAAAAAgAwGMhXMwxV2EKAAAAZAAAACcAAABhLW5hbWUtbG9uZ2VyLXRoYW4tdHdlbnR5LXR3by1j
aGFycy50eHTLyc9L58qhLQEAUEsBAhQDFAAAAAAAwGMhXAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAQ
AO1BAAAAAGQvUEsBAhQDFAAAAAgAwGMhXLoPAE0QAAAA8AAAAAcAAAAAAAAAAAAAAKSBIAAAAGQv
eC50eHRQSwECFAMUAAAAAADAYyFcAAAAAAAAAAAAAAAABAAAAAAAAAAAABAA7UFVAAAAZC9zL1BL
AQIUAxQAAAAAAMBjIVxsoUm6AwAAAAMAAAAJAAAAAAAAAAAAAACkgXcAAABkL3MveS50eHRQSwEC
FAMUAAAAAADAYyFcIvR4SQQAAAAEAAAABwAAAAAAAAAAAAAApIGhAAAAdG9wLnR4dFBLAQIUAxQA
AAAAAMBjIVwAAAAAAAAAAAAAAAAFAAAAAAAAAAAAAACkgcoAAABlLnR4dFBLAQIUAxQAAAAAAMBj
IVxOgYhHBAAAAAQAAAAKAAAAAAAAAAAAAACkge0AAABzcCBhY2UudHh0UEsBAhQDFAAAAAgAwGMh
XMwxV2EKAAAAZAAAACcAAAAAAAAAAAAAAKSBGQEAAGEtbmFtZS1sb25nZXItdGhhbi10d2VudHkt
dHdvLWNoYXJzLnR4dFBLBQYAAAAACAAIAMMBAABoAQAAAAA=
EOF2
base64 -d > slip.zip <<'EOF2'
UEsDBBQAAAAAAMBjIVx6zT+3BQAAAAUAAAALAAAALi4vZXZpbC50eHRldmlsClBLAwQUAAAAAADA
YyFcH5NKDQQAAAAEAAAACAAAAC9hYnMudHh0YWJzClBLAwQUAAAAAADAYyFcfQ4W2gMAAAADAAAA
BgAAAG9rLnR4dG9rClBLAwQUAAAAAADAYyFcHwjqRgIAAAACAAAADQAAAGEvLi4vLi4veC50eHR4
ClBLAQIUAxQAAAAAAMBjIVx6zT+3BQAAAAUAAAALAAAAAAAAAAAAAACkgQAAAAAuLi9ldmlsLnR4
dFBLAQIUAxQAAAAAAMBjIVwfk0oNBAAAAAQAAAAIAAAAAAAAAAAAAACkgS4AAAAvYWJzLnR4dFBL
AQIUAxQAAAAAAMBjIVx9DhbaAwAAAAMAAAAGAAAAAAAAAAAAAACkgVgAAABvay50eHRQSwECFAMU
AAAAAADAYyFcHwjqRgIAAAACAAAADQAAAAAAAAAAAAAApIF/AAAAYS8uLi8uLi94LnR4dFBLBQYA
AAAABAAEAN4AAACsAAAAAAA=
EOF2
unzip -l a.zip; echo "unzip -l rc=$?"
unzip -lq a.zip | head -3
unzip -qql a.zip | head -2
unzip -l a.zip 'd/*'; echo "unzip -l pattern rc=$?"
unzip -l a.zip nomatch; echo "unzip -l nomatch rc=$?"
unzip -Z1 a.zip
unzip -Z1 a.zip '*.txt'
unzip a.zip -d out; echo "unzip rc=$?"; find out | sort
unzip a.zip -d out 2>&1; echo "unzip again rc=$?"
unzip -o a.zip -d out; echo "unzip -o rc=$?"
unzip -n a.zip -d out; echo "unzip -n rc=$?"
unzip -q a.zip -d out 2>&1; echo "unzip -q again rc=$?"
unzip -oq a.zip -d out; echo "unzip -oq rc=$?"
unzip -oqq a.zip -d out; echo "unzip -oqq rc=$?"
unzip a.zip -d fresh1/sub 2>&1; echo "unzip -d missing parent rc=$?"
unzip -j a.zip -d flat; echo "unzip -j rc=$?"; find flat | sort
unzip a.zip top.txt e.txt -d some; echo "unzip members rc=$?"; find some | sort
unzip a.zip 'd/*' -d pat; echo "unzip d/* rc=$?"; find pat | sort
unzip a.zip 'd/**' -d pat2 | tail -3; find pat2 | sort
unzip a.zip -x 'd/*' -d excl | tail -3; find excl | sort
unzip a.zip nomatch -d none 2>&1; echo "unzip nomatch rc=$?"
unzip -p a.zip top.txt e.txt sp\ ace.txt; echo "unzip -p rc=$?"
unzip -pq a.zip d/s/y.txt
unzip -t a.zip; echo "unzip -t rc=$?"
unzip -tq a.zip; echo "unzip -tq rc=$?"
unzip -tqq a.zip; echo "unzip -tqq rc=$?"
unzip -t a.zip nomatch; echo "unzip -t nomatch rc=$?"
unzip -d slip slip.zip 2>&1; echo "unzip slip rc=$?"; find slip | sort; ls ../evil.txt ../abs.txt 2>&1 | wc -l
unzip nonexistent 2>&1; echo "unzip nonexistent rc=$?"
echo notazip > notzip.zip; unzip notzip.zip 2>&1; echo "unzip notzip rc=$?"
cp a.zip noext; unzip -l noext | head -2; unzip -l a | head -2
unzip -l a.zip a.zip 2>&1 | tail -3
unzip -Q a.zip 2>&1 | head -3; unzip -Q a.zip >/dev/null 2>&1; echo "unzip -Q rc=$?"
unzip 2>&1 | head -4; echo "unzip no args rc=${PIPESTATUS[0]}"
