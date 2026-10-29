# M3-SH12: xxd as Git Bash's (vim xxd): dumps, groups, columns, offsets, plain, C include,
# bits, little endian, autoskip, reverts, and the usage/exit-code behaviour.
printf 'Hello, World!\nsecond line here ok\n\x00\xff\x7f~' > f.bin
xxd f.bin
xxd -l 5 f.bin
xxd -s 3 -l 5 f.bin
xxd -s -4 f.bin
xxd -c 8 f.bin
xxd -g1 -l 20 f.bin
xxd -g4 -l 20 f.bin
xxd -g0 -l 20 f.bin
xxd -c4 -g3 -l 10 f.bin
xxd -u -l 20 f.bin
xxd -b -l 8 f.bin
xxd -e -l 16 f.bin
xxd -E -l 16 f.bin
xxd -d -l 20 f.bin
xxd -o 0x100 -l 20 f.bin
xxd -p f.bin
xxd -p -c 5 -l 12 f.bin
xxd -ps -u -l 6 f.bin
xxd -i f.bin
xxd -i -c4 -l 9 f.bin
xxd -i -u -l 3 f.bin
xxd -i -C -n foo -l 3 f.bin
printf abc | xxd -i
printf abc | xxd -i -n z
head -c 12 f.bin | xxd -i -t | tail -2
head -c 100 /dev/zero > z.bin; printf abc >> z.bin
xxd -a z.bin
xxd -a -c 8 z.bin
printf 'abc' | xxd
printf '' | xxd; echo "empty rc=$?"
xxd -l 0 f.bin; echo "l0 rc=$?"
# round trips
xxd f.bin | xxd -r | cmp - f.bin && echo "r ok"
xxd -p f.bin | xxd -r -p | cmp - f.bin && echo "r -p ok"
xxd -b -l 6 f.bin | xxd -r -b | od -An -c
printf '0000000: 4142 4344  ABCD\n' | xxd -r | od -An -c
printf '0: 41 42\n8: 43\n' | xxd -r | od -An -c
printf '0: 41 42\n8: 43\n' | xxd -r -s 4 | od -An -c
printf '4142 43 44\n' | xxd -r -p | od -An -c
printf '0: 4142\n' > hx.txt; xxd -r hx.txt out.bin; printf '2: 44\n' | xxd -r - out.bin; od -An -c out.bin
# errors and exit codes
xxd nonexist; echo "rc=$?"
printf '0: 41\n0: 42\n' | xxd -r 2>&1 | od -An -c | head -3
printf '0: 41\n' | xxd -r -i 2>&1; echo "rc=$?"
xxd -c 257 f.bin; echo "rc=$?"
xxd -b -p f.bin; echo "rc=$?"
xxd -e -g3 f.bin; echo "rc=$?"
xxd -Z f.bin 2>&1 | head -1; xxd -Z f.bin >/dev/null 2>&1; echo "rc=$?"
xxd -h 2>&1 | head -3
xxd -v 2>&1
xxd a b c 2>&1 | head -1; echo "rc=${PIPESTATUS[0]}"
printf abcdefgh | xxd -s -3 2>&1; echo "rc=$?"
printf abc | xxd -s 10 2>&1; echo "rc=$?"
xxd -R always -l 20 f.bin | od -An -c | head -4
