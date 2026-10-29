# M3-SH12: column as Git Bash's (util-linux 2.40): fill mode, table mode, separators, headers,
# alignment, hiding, ordering, JSON, and the error texts.
export COLUMNS=80
column -t data.csv -s,
column -t -s, -o ' | ' data.csv
column -t -s, -N NAME,AGE -R AGE data.csv
column -t -s, -N NAME,AGE -H AGE data.csv
column -t -s, -N NAME,AGE -O 2,1 data.csv
column -t -s, -N NAME,AGE -d data.csv
column -J -s, -N name,age data.csv
column -J -s, -N name,age -n people data.csv
printf 'a b c\n1 2 3\n' | column -t -J -N x,y,z -H y
printf 'a\tb c\td\naa\tbbb\t\n' | column -t -s "$(printf '\t')"
printf 'a  b   c\n\n  lead x\n' | column -t
printf 'a  b\n\n c d\n' | column -t -L
printf 'a:b:\n:\nlast\n' | column -t -s:
printf 'a b c d e\n' | column -t -l 3
printf 'x \xc3\xa9 y\nlonger \xe6\x97\xa5\xe6\x9c\xac z\n' | column -t
seq 1 30 | column
seq 1 30 | column -c 40
seq 1 30 | column -x -c 40
seq 1 12 | column -c 20
printf 'abcdefghij\nxy\n' | column -c 5
printf 'one\n\n  \ntwo\n' | column -c 80 | od -An -c
printf 'one\n\ntwo\n' | column -L -c 80 | od -An -c
printf 'a b\n' | column -t -m -c 40
column nofile; echo "rc=$?"
column -t nofile data.csv -s, | head -2; echo "rc=${PIPESTATUS[0]}"
column --bogus 2>&1; echo "rc=$?"
column -c 2>&1; echo "rc=$?"
column -c abc a.txt 2>&1; echo "rc=$?"
column -t -H 9 a.txt 2>&1; echo "rc=$?"
column -J a.txt 2>&1; echo "rc=$?"
column --out=5 a.txt 2>&1 | head -1
column -V
column -h | head -3
column -t -s, -o '||' --table-columns n,a data.csv
