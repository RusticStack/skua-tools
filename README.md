# skua-tools

`file`, `dos2unix`, `unix2dos`, `clear`, `ps`, `pgrep`, `pkill`, `gzip`/`gunzip`/`zcat`, `unzip`, `zip`, `xxd`, `column`, `iconv`, `start`, `egrep`, `fgrep` and `perl` for Skua Bash, the bash-compatible shell bundled with Skua on
Windows. Git Bash ships these (libmagic `file` 5.4x, dos2unix 7.x) and coding agents use them,
for example `git ls-files | xargs file` to see line endings. No existing Rust implementation gave
Git Bash's output, so these are small native programs written for that output.

- `file`: text (ASCII, UTF-8, UTF-8 with BOM, ISO-8859, extended ASCII; CRLF/CR/LF terminators,
  no terminators, very long lines, escape sequences), scripts by `#!` (sh, bash, python, node,
  perl, ...), JSON, XML, HTML, PE/ELF executables, PNG, GIF, JPEG, PDF, archives, `empty`,
  `directory`, symbolic links; anything else is `data`. Options `-b -L -h -i -N -F -f
  --mime-type --mime-encoding`. Not done: libmagic's source-language guesses (`C source`),
  gzip header details.
- `dos2unix` / `unix2dos` (one program; the name picks the direction): in place, `-n IN OUT`,
  stdin to stdout, `-q -k -f -b -r -m`, binary files skipped, dos2unix 7.x messages.

- `clear`: Git Bash's ncurses `clear` (`\e[H\e[2J\e[3J` for xterm-like terminals, `-x`, `-T`, `cygwin`/`screen`/`dumb`, the same errors); names Git Bash's terminfo lacks are cleared like xterm.
- `ps`: Git Bash's (Cygwin) `ps` table layouts (default, `-l`, `-f`, `-s`, `aux`, `-e`, `-p`, `-u`) over all Windows processes (PID = Windows PID).
- `pgrep` / `pkill` (one code base): procps-ng behaviour (Git Bash has neither). `-f -l -a -x -i -n -o -c -v -u -P ...`, exit 0/1/2/3. Signals that end a process on Unix all terminate it; STOP/TSTP are refused. Never matches itself or an ancestor.
- `gzip` / `gunzip` / `zcat` (one program, the name picks the mode): GNU gzip 1.14 messages and exit codes; compressed bytes and sizes differ (miniz_oxide). No `.Z`.
- `unzip`: Info-ZIP UnZip 6.00 output (`-l -v -Z -t -p -d -o -n -j -x ...`). bzip2, deflate64, LZMA and encrypted entries are skipped.
- `zip`: Info-ZIP Zip 3.0 behaviour from its documentation (Git for Windows has no zip, so no reference run): `-r -j -D -q -0..-9 -x -i -u -f -d -m -@ -T`. No Zip64 writing, encryption or comments.
- `xxd`: byte-for-byte with Git Bash's xxd (all dump modes and `-r`).
- `column`: util-linux 2.40 `column` (fill, table and JSON modes).
- `iconv`: GNU libiconv 1.19 behaviour for UTF-8/16/32, UCS, ASCII, Latin-1, Windows-125x, ISO-8859-x, `//TRANSLIT`, `//IGNORE`; CJK and a few others follow WHATWG tables.
- `start`: `cmd.exe /c start "" ARGS` with complete escaping; `SKUA_START_DRY_RUN=1` prints instead of running.
- `egrep` / `fgrep`: `grep -E` / `grep -F` launchers (the sibling `grep.exe` first, else `grep` on PATH).
- `perl`: launcher only. Runs Git for Windows' perl (`SKUA_PERL`, then Git found on PATH, then `C:\Program Files\Git\usr\bin\perl.exe`).

Built by Skua Bash's `skua/build-uutils.ps1` (entry in `skua/uutils.toml`). MIT.
