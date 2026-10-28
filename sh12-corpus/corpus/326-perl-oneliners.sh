# M3-SH12 misc: perl one-liners agents run: in-place edits (-pi -e, with backup suffix, several
# files, MSYS and Windows paths), -ne/-lane filters on stdin, exit codes, die/warn, -v banner.
cp a.txt p1.txt; perl -pi -e 's/alpha/ALPHA/' p1.txt; cat p1.txt
cp a.txt p2.txt; perl -pi.bak -e 's/beta/BETA/' p2.txt; cat p2.txt; cat p2.txt.bak | head -2
cp a.txt p3.txt; cp b.txt p4.txt; perl -pi -e 's/\bo/0/g; s/^/> /' p3.txt p4.txt; cat p3.txt p4.txt
cp a.txt p5.txt; perl -pi -e "s/gamma/GAMMA/" "$(cygpath -w "$PWD/p5.txt")"; cat p5.txt | sed -n 3p
cp a.txt p6.txt; perl -i -ne 'print unless /alpha/' p6.txt; cat p6.txt
perl -ne 'print if /a$/' a.txt
printf 'a b c\nd e f\n' | perl -lane 'print $F[1]'
printf '1\n2\n3\n' | perl -ne '$s += $_; END { print "sum=$s\n" }'
printf 'x\ny\n' | perl -pe 'chomp; $_ .= "!\n"'
perl -le 'print scalar(@ARGV), " args: @ARGV"' one "two words" three
perl -e 'print "no newline"'; echo
perl -e 'exit 7'; echo "exit rc=$?"
perl -e 'die "dying\n"' 2>&1; echo "die rc=$?"
perl -e 'warn "warned\n"; print "after\n"' 2>&1; echo "warn rc=$?"
perl -e 'print STDERR "to stderr\n"; print "to stdout\n"' 2>/dev/null
perl -v | sed -n 2p | sed 's/(v[0-9.]*)/(vX)/; s/version [0-9]*, subversion [0-9]*/version N/'
perl -pi -e 's/x/y/' no-such-file-here.txt 2>&1 | sed 's/ at .*//'; echo "missing rc=${PIPESTATUS[0]}"
perl -MPOSIX -e 'print POSIX::floor(3.7), "\n"'
perl -MList::Util=sum,max -e 'print sum(1..4), " ", max(3,9,2), "\n"'
perl -e 'print join(",", sort { $a <=> $b } (10, 9, 100)), "\n"'
