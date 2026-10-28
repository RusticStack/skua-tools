# M3-SH12: pkill (procps). Git Bash has no pkill (command not found, 127), so this case is not
# recorded from it: the expected output is procps-ng's semantics on Windows processes the case
# starts. TERM, KILL, INT and HUP all end the process on Windows; signal 0 only checks.
gone() { sleep 0.5; }
for n in 7321 7322 7323 7324; do sleep $n & sleep 0.2; done
sleep 0.3
pkill -0 -f "sleep 7321"; echo "pkill -0 rc=$?"
pgrep -fc "sleep 7321"
pkill -f "sleep 7321"; echo "pkill rc=$?"
gone
pgrep -fc "sleep 7321"
pkill -f "sleep 7321"; echo "pkill again rc=$?"
pkill -9 -f "sleep 7322"; echo "pkill -9 rc=$?"
gone
pgrep -fc "sleep 7322"
pkill -e -f "sleep 7323" | sed 's/(pid [0-9]*)/(pid N)/'
gone
pkill -KILL -f "sleep 7323"; echo "pkill nothing left rc=$?"
pkill --signal HUP -c -f "sleep 7324"; echo "pkill --signal HUP -c rc=$?"
gone
pgrep -fc "sleep 732[1-4]"
sleep 7325 &
sleep 0.3
sleep 7326 &
sleep 0.3
pkill -INT -n -f "sleep 732[56]"; echo "pkill -n rc=$?"
gone
pgrep -f "sleep 732[56]" | wc -l
pgrep -fl "sleep 732[56]" | sed 's/^[0-9][0-9]* //'
pkill -SIGTERM -o -f "sleep 732[56]"; echo "pkill -o rc=$?"
gone
pgrep -fc "sleep 732[56]"
sleep 7327 &
sleep 0.3
pkill -x slee; echo "pkill -x partial rc=$?"
pgrep -fc "sleep 7327"
pkill -f "sleep 7327" -u "$USERNAME"; echo "pkill -u own rc=$?"
gone
pgrep -fc "sleep 7327"
pkill 2>&1; echo "no pattern rc=$?"
pkill -f 2>&1 | head -1; echo "missing pattern rc=${PIPESTATUS[0]}"
pkill -z x 2>&1 | head -1; echo "bad option rc=${PIPESTATUS[0]}"
pkill --signal FOO x 2>&1; echo "bad signal rc=$?"
pkill --help | head -3
pkill --help > /dev/null; echo "help rc=$?"
pkill -f "pkill -f nothing like this 7329"; echo "self rc=$?"
