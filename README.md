# skua-tools

`file`, `dos2unix`, `unix2dos`, `clear`, `ps`, `pgrep`, `pkill`, `gzip`/`gunzip`/`zcat`, `unzip`, `zip`, `xxd`, `column`, `iconv`, `start`, `egrep`, `fgrep`, `perl` and `id` for Skua Bash, the bash-compatible shell bundled with Skua on
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
- `id`: Git Bash's (GNU coreutils 8.32 on Cygwin) `id` with Git for Windows' default `nsswitch.conf`: uid/gid from the process token with Cygwin's SID mapping (`0x30000 + RID` for local accounts, well-known SIDs fixed), user names from Windows accounts, no group names (`cannot find name for group ID`, status 1), the current user's groups = the primary group. All options, errors and statuses as Git Bash's; `--version` says `id (skua-tools)`.
- `perl`: launcher only. Runs Git for Windows' perl: `SKUA_PERL`, else the first Git for Windows install found (`git.exe` on PATH, `%ProgramFiles%\Git`, `%LOCALAPPDATA%\Programs\Git`, `InstallPath` under `HKLM`/`HKCU\SOFTWARE\GitForWindows`, Scoop) that has `perl.exe` in `usr\bin`, `ucrt64\bin`, `mingw64\bin` or `clangarm64\bin`. An install is recognised by `cmd\git.exe` only (the path Git for Windows promises to keep; 2.56 moved `mingw64` to `ucrt64`). Not found: one line saying what was looked for, status 127. The lookup is `skua_tools::gitwin`.

Built by Skua Bash's `skua/build-uutils.ps1` (entry in `skua/uutils.toml`). MIT.
