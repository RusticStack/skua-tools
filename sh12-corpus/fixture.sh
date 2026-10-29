#!/bin/bash
# M3-SH0 corpus fixture: run by Git Bash in an empty sandbox dir before every case, so the shell
# under test and the Git Bash reference start from the same files and the same git hashes.
set -e
mkdir -p src docs "dir with space" .hidden
printf 'alpha\nbeta\ngamma\nalpha\n' > a.txt
printf 'one two three\nfour five six\n' > b.txt
printf 'fn main() {\n    println!("hello");\n}\n' > src/main.rs
printf 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n// TODO: tests\n' > src/lib.rs
printf '# Title\n\nSome docs.\nTODO: write more\n' > docs/README.md
printf 'x\n' > "dir with space/file one.txt"
printf 'secret=no\n' > .hidden/config
printf 'name,age\nann,31\nbob,27\ncid,45\n' > data.csv
touch zqcompletion_target.txt
git init -q -b main
git config core.autocrlf false
git config user.name Test
git config user.email t@example.com
git add -A
git commit -q -m "initial commit"
echo delta >> a.txt
git commit -q -am "add delta"
echo epsilon >> a.txt
