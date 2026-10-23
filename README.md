# skua-tools

`file`, `dos2unix` and `unix2dos` for Skua Bash, the bash-compatible shell bundled with Skua on
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

Built by Skua Bash's `skua/build-uutils.ps1` (entry in `skua/uutils.toml`). MIT.
