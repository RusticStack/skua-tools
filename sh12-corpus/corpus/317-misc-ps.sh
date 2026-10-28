# M3-SH12: ps in Git Bash's layout. Process listings are machine-specific, so only the headers,
# one process the case starts (found by its command line), filters, usage errors and exit
# statuses are shown. The PID is read from `ps -f`: Skua Bash's $! is not the process id.
ps | head -1
ps -l | head -1
ps -W | head -1
ps -e | head -1
ps aux | head -1
ps -ef | head -1
ps -f | head -1
ps -s | head -1
sleep 7301 &
sleep 0.5
pid=$(ps -f | awk '/sleep 730[1]/ { print $2; exit }')
ps -p $pid | wc -l
ps -p $pid | awk -v p=$pid 'NR == 2 { print ($1 == p) ? "pid column ok" : "pid column wrong" }'
ps -p $pid | awk -v p=$$ 'NR == 2 { print ($2 == p) ? "ppid column ok" : "ppid column wrong" }'
ps -p $pid | awk 'NR == 2 { print ($NF ~ /sleep/) ? "command ok" : "command wrong" }'
ps -p $pid | awk 'NR == 2 { print (NF >= 8) ? "columns ok" : "columns wrong" }'
ps -s -p $pid | head -1
ps -s -p $pid | awk 'NR == 2 { print (NF >= 4) ? "short columns ok" : "short columns wrong" }'
ps -f -p $pid | awk -v p=$pid 'NR == 2 { print ($2 == p) ? "full pid ok" : "full pid wrong" }'
ps -fl -p $pid | head -1
ps -ef | awk -v p=$pid '$2 == p { n++ } END { print n " listed with -ef" }'
ps aux | awk -v p=$pid '$1 == p || $2 == p { n++ } END { print n " listed with aux" }'
ps -p 99999999 | wc -l
ps -p 99999999 > /dev/null; echo "ps -p none rc=$?"
ps -u 99999999 | wc -l
ps -u 99999999 > /dev/null; echo "ps -u none rc=$?"
ps -x 2>&1; echo "ps -x rc=$?"
ps --bogus 2>&1; echo "ps --bogus rc=$?"
ps -p 2>&1; echo "ps -p rc=$?"
ps -u 2>&1; echo "ps -u rc=$?"
ps --help | head -1
ps --help > /dev/null; echo "ps --help rc=$?"
ps < /dev/null | head -1
kill $pid
sleep 0.5
ps -p $pid | wc -l
