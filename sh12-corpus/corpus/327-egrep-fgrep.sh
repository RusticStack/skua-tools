# M3-SH12 misc: egrep is `grep -E`, fgrep is `grep -F` (Git Bash: two-line `exec grep -E "$@"`
# scripts, no warning): same output, stdin, usage error text and exit status.
egrep 'alpha|gamma' a.txt; echo "rc=$?"
egrep -c '^(one|four)' b.txt; echo "rc=$?"
egrep -n 'a(l|m)+' a.txt; echo "rc=$?"
egrep -r --include='*.rs' 'fn (main|add)' src | sort; echo "rc=$?"
fgrep 'a+b' src/lib.rs; echo "rc=$?"
fgrep -n '.rs' a.txt data.csv; echo "rc=$?"
fgrep -F -x alpha a.txt; echo "rc=$?"
printf 'a.c\nabc\n' | fgrep 'a.c'; echo "rc=$?"
printf 'a.c\nabc\n' | egrep 'a.c'; echo "rc=$?"
printf 'x\n' | egrep -q y; echo "quiet no match rc=$?"
egrep zzz a.txt; echo "no match rc=$?"
egrep alpha nosuchfile 2>&1; echo "missing rc=$?"
egrep 2>&1; echo "usage rc=$?"
fgrep 2>&1; echo "usage rc=$?"
egrep --version 2>&1 | head -1 | sed 's/[0-9][0-9.]*/N/g'
fgrep -e alpha -e beta a.txt | sort -u
egrep -o '[a-z]+' b.txt | head -3
egrep -i 'ALPHA' a.txt | head -1
egrep -w 'a' a.txt; echo "rc=$?"
