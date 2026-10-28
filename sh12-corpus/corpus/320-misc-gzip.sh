# M3-SH12 (zip group): gzip as Git Bash's gzip 1.14 behaves for agents: compress in place (the
# .gz replaces the file), -k -f -c -d -t -l, gunzip/zcat, stdin to stdout, refusals and their
# wording (stderr merged where it matters), exit codes 1 (error) and 2 (warning). Compressed
# bytes and sizes are not compared (the compressor differs); -l is checked on a fixed file.
printf 'alpha\nbeta\n%.0s' {1..50} > g.txt; cp g.txt g.orig
gzip g.txt; echo "gzip rc=$?"; ls g.txt g.txt.gz 2>&1
gzip -d g.txt.gz; echo "gzip -d rc=$?"; cmp g.txt g.orig && echo roundtrip-ok; ls g.txt.gz 2>&1
gzip -k g.txt; echo "gzip -k rc=$?"; ls g.txt g.txt.gz
gzip g.txt 2>&1; echo "gzip again rc=$?"; ls g.txt g.txt.gz
gzip -f g.txt; echo "gzip -f rc=$?"; ls g.txt g.txt.gz 2>&1
gzip -dk g.txt.gz 2>&1; echo "gzip -dk rc=$?"
gzip -dkf g.txt.gz; echo "gzip -dkf rc=$?"; cmp g.txt g.orig && echo kept-ok
gzip -dc g.txt.gz | cmp - g.orig && echo dc-ok
gzip -c g.orig | gzip -dc | cmp - g.orig && echo pipe-ok
gunzip -c g.txt.gz | cmp - g.orig && echo gunzip-ok
zcat g.txt.gz | head -2
for l in 1 6 9; do gzip -$l -c g.orig | gzip -dc | wc -c; done
printf 'hello\n' | gzip | gzip -dc
printf '' | gzip | gzip -dc | wc -c
gzip -t g.txt.gz; echo "gzip -t rc=$?"
printf 'junk' > bad.gz; printf 'junk\n' > plain.txt
gzip -t bad.gz 2>&1; echo "gzip -t bad rc=$?"
gzip -dc bad.gz 2>&1; echo "gzip -dc bad rc=$?"
gzip -d plain.txt 2>&1; echo "gzip -d plain rc=$?"
gzip -dc plain.txt 2>&1; echo "gzip -dc plain rc=$?"
gzip -dcf plain.txt; echo "gzip -dcf plain rc=$?"
gzip -dc nonexist 2>&1; echo "gzip -dc nonexist rc=$?"
gzip nonexist 2>&1; echo "gzip nonexist rc=$?"
gzip -q nonexist 2>&1; echo "gzip -q nonexist rc=$?"
gzip g.txt.gz 2>&1; echo "gzip x.gz rc=$?"
echo ro > ro.txt; chmod a-w ro.txt; gzip ro.txt; echo "gzip read-only rc=$?"; ls ro.txt ro.txt.gz 2>&1; ls -l ro.txt.gz | cut -c1-10
gzip -d ro.txt.gz; echo "gzip -d read-only rc=$?"; ls ro.txt ro.txt.gz 2>&1; ls -l ro.txt | cut -c1-10
mkdir dd; echo z > dd/f; echo q > dd/g
gzip dd 2>&1; echo "gzip dir rc=$?"
gzip -r dd; echo "gzip -r rc=$?"; ls dd
gzip -dr dd; echo "gzip -dr rc=$?"; ls dd
echo one > s1; echo two > s2; gzip -S .zz s1 s2; echo "gzip -S rc=$?"; ls s1* s2*
gzip -d -S .zz s1.zz s2.zz; echo "gzip -d -S rc=$?"; cat s1 s2
base64 -d > fixed.gz <<'EOF2'
H4sIAAAAAAACA0vMKchI5EpKLUnkShxljjKxMwGOzv7QJgIAAA==
EOF2
gzip -l fixed.gz
gzip -lq fixed.gz
gzip -t fixed.gz; echo "fixed -t rc=$?"
gzip -dc fixed.gz | cmp - g.orig && echo fixed-ok
gzip -lq fixed.gz fixed.gz
gzip -x 2>&1; echo "gzip -x rc=$?"
gzip --bogus 2>&1; echo "gzip --bogus rc=$?"
gzip -S 2>&1; echo "gzip -S rc=$?"
gzip --help | head -2
gzip -V | head -1
