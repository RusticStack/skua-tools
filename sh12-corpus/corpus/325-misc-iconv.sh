# M3-SH12: iconv as Git Bash's (GNU libiconv 1.19): Unicode forms, Latin-1, Windows-1252,
# //TRANSLIT, //IGNORE, -c, -s, the substitution options, and the error messages and exit codes.
printf 'h\xc3\xa9llo \xe2\x82\xac \xf0\x9f\x98\x80\n' > u8.txt
for t in UTF-16LE UTF-16BE UTF-16 UTF-32 UCS-2LE UCS-4BE UCS-2; do echo "== $t"; iconv -f UTF-8 -t $t u8.txt | od -An -tx1; done
iconv -f UTF-8 -t LATIN1//TRANSLIT u8.txt
iconv -f UTF-8 -t ASCII//TRANSLIT u8.txt
iconv -f UTF-8 -t ASCII//IGNORE u8.txt
iconv -f UTF-8 -t CP1252//TRANSLIT u8.txt | od -An -tx1
iconv -c -f UTF-8 -t LATIN1 u8.txt | od -An -tx1; echo "c rc=${PIPESTATUS[0]}"
iconv -f UTF-8 -t LATIN1 u8.txt 2>&1 | od -An -c | head -3
iconv -f UTF-8 -t ASCII u8.txt 2>&1; echo "rc=$?"
iconv -s -f UTF-8 -t ASCII u8.txt; echo "s rc=$?"
printf 'h\xe9llo \x80 \xff\n' > l1.txt
iconv -f LATIN1 -t UTF-8 l1.txt
iconv -f CP1252 -t UTF-8 l1.txt
iconv -f WINDOWS-1252 -t UTF-8 l1.txt
iconv -f ISO-8859-15 -t UTF-8 l1.txt
printf '\x81' | iconv -f CP1252 -t UTF-8 2>&1; echo "rc=$?"
printf 'h\0\xe9\0\n\0' | iconv -f UTF-16LE -t UTF-8
printf '\xff\xfeh\0\xe9\0\n\0' | iconv -f UTF-16 -t UTF-8
printf '\xfe\xff\0h\0\xe9\0\n' | iconv -f UTF-16 -t UTF-8
printf 'h\0\xe9\0\n' | iconv -f UTF-16LE -t UTF-8 2>&1; echo "rc=$?"
printf 'l1\nab\xc3(\n' | iconv -f UTF-8 -t UTF-16LE 2>&1 | od -An -c | head -4
printf 'ab\xc3(\n' | iconv -c -f UTF-8 -t UTF-8; echo "rc=$?"
printf 'ab\xc3' | iconv -f UTF-8 -t UTF-8 2>&1; echo "rc=$?"
printf '\xef\xbb\xbfab\n' | iconv -f UTF-8 -t UTF-16LE | od -An -tx1
printf '\xe2\x80\x9cq\xe2\x80\x9d \xe2\x80\x94 \xe2\x80\xa6 \xc2\xa9 \xc3\x9f \xc5\x93 \xc2\xb1\xc2\xbd\n' | iconv -f UTF-8 -t ASCII//TRANSLIT
printf 'a\xe4\xb8\xadb\n' | iconv -f UTF-8 -t ASCII//TRANSLIT 2>&1; echo "rc=$?"
printf 'a\xe4\xb8\xadb\xc3(\n' | iconv --unicode-subst='<U+%04X>' -f UTF-8 -t ASCII 2>&1; echo "rc=$?"
printf 'a\xc3(\n' | iconv --byte-subst='<0x%02X>' -f UTF-8 -t ASCII; echo "rc=$?"
printf 'a\xe4\xb8\xadb\n' | iconv --unicode-subst=%d -f UTF-8 -t ASCII 2>&1; echo "rc=$?"
printf 'x\n' | iconv -f UTF-8 -t UTF-16LE -- - | od -An -tx1
printf 'x\n' | iconv --from-code=UTF-8 --to-code=UTF-16LE | od -An -tx1
printf 'x\n' | iconv --from-code UTF-8 --to-code UTF-16LE | od -An -tx1
printf 'x\n' | iconv -f utf-8 -t Latin1; echo "rc=$?"
printf 'x\n' | iconv -f UTF8 -t ASCII 2>&1; echo "rc=$?"
printf 'x\n' | iconv -f UTF-8 -t ASCII//FOO 2>&1 | head -1
printf 'x\n' | iconv -f BOGUS -t UTF-8 2>&1; echo "rc=$?"
iconv -f UTF-8 -t ASCII nofile; echo "rc=$?"
iconv -z 2>&1; echo "rc=$?"
iconv -f 2>&1 | head -1
printf 'x' | iconv -fUTF-8 -tASCII 2>&1 | head -1
printf 'x\n' | iconv -f UTF-8 -t ASCII u8.txt - 2>&1 | head -2
iconv --version | head -1
iconv --help | head -2
iconv -l | head -2
iconv -l | grep -c -E '^UTF-16LE$'
