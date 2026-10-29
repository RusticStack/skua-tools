# M3-SH12: pgrep (procps). Git Bash has no pgrep (command not found, 127), so this case is
# not recorded from it: the expected output is procps-ng's semantics on Windows processes the
# case starts. -f patterns use unique sleep times; PIDs come from pgrep itself.
sleep 7311 &
sleep 0.3
sleep 7312 &
sleep 0.5
p1=$(pgrep -f "sleep 7311")
p2=$(pgrep -f "sleep 7312")
echo "$p1" | awk '{ print ($1 + 0 > 0 && NF == 1) ? "one pid for 7311" : "wrong" }'
pgrep -f "sleep 731[12]" | wc -l
pgrep -fc "sleep 731[12]"
pgrep -fn "sleep 731[12]" | awk -v p=$p2 '{ print ($1 == p) ? "newest ok" : "newest wrong" }'
pgrep -fo "sleep 731[12]" | awk -v p=$p1 '{ print ($1 == p) ? "oldest ok" : "oldest wrong" }'
pgrep -fd, "sleep 731[12]" | tr ',' '\n' | sort -n | wc -l
pgrep -fd, "sleep 731[12]" | awk '{ print (NF == 1 && index($0, ",") > 0) ? "delimiter ok" : "delimiter wrong" }'
pgrep -fl "sleep 7311" | sed 's/^[0-9][0-9]* //'
pgrep -fa "sleep 7311" | sed 's/^[0-9][0-9]* //'
pgrep -x sleep | awk -v p=$p1 '$1 == p { print "name match ok" }'
pgrep -x SLEEP | wc -l
pgrep -ix SLEEP | awk -v p=$p1 '$1 == p { print "ignore-case ok" }'
pgrep -x slee | wc -l
pgrep -fv "sleep 7311" | awk -v p=$p1 '$1 == p { print "inverse wrong" }'
pgrep -P $$ -f "sleep 7311" | awk -v p=$p1 '{ print ($1 == p) ? "parent ok" : "parent wrong" }'
pgrep -P 1 -f "sleep 7311" | wc -l
pgrep -f "sleep 7311" > /dev/null; echo "match rc=$?"
pgrep -f "no such process 73119" > /dev/null; echo "no match rc=$?"
pgrep -fc "no such process 73119"
pgrep 2>&1; echo "no pattern rc=$?"
pgrep -z x 2>&1 | head -1; echo "bad option rc=${PIPESTATUS[0]}"
pgrep a b 2>&1 | head -1; echo "two patterns rc=${PIPESTATUS[0]}"
pgrep -d 2>&1 | head -1
pgrep -f "(" > /dev/null 2>&1; echo "bad regex rc=$?"
pgrep --help | head -3
pgrep --help > /dev/null; echo "help rc=$?"
pgrep -f "pgrep -f nothing like this 7319" > /dev/null; echo "self rc=$?"
pgrep < /dev/null > /dev/null 2>&1; echo "no stdin read rc=$?"
kill $p1
kill $p2
sleep 0.5
pgrep -f "sleep 731[12]" | wc -l
