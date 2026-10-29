//! getopt_long-style parsing for `column` (util-linux 2.40 as built for Git for Windows):
//! options may follow the files, long options may be abbreviated, `--` ends the options, and
//! the error texts are the Cygwin getopt ones.

pub const HELP: &str = "
Usage:
 column [options] [<file>...]

Columnate lists.

Options:
 -t, --table                      create a table
 -n, --table-name <name>          table name for JSON output
 -O, --table-order <columns>      specify order of output columns
 -C, --table-column <properties>  define column
 -N, --table-columns <names>      comma separated columns names
 -l, --table-columns-limit <num>  maximal number of input columns
 -E, --table-noextreme <columns>  don't count long text from the columns to column width
 -d, --table-noheadings           don't print header
 -m, --table-maxout               fill all available space
 -e, --table-header-repeat        repeat header for each page
 -H, --table-hide <columns>       don't print the columns
 -R, --table-right <columns>      right align text in these columns
 -T, --table-truncate <columns>   truncate text in the columns when necessary
 -W, --table-wrap <columns>       wrap text in the columns when necessary
 -L, --keep-empty-lines           don't ignore empty lines
 -J, --json                       use JSON output format for table

 -r, --tree <column>              column to use tree-like output for the table
 -i, --tree-id <column>           line ID to specify child-parent relation
 -p, --tree-parent <column>       parent to specify child-parent relation

 -c, --output-width <width>       width of output in number of characters
 -o, --output-separator <string>  columns separator for table output (default is two spaces)
 -s, --separator <string>         possible table delimiters
 -x, --fillrows                   fill rows before columns

 -h, --help                       display this help
 -V, --version                    display version

For more details see column(1).
";

pub const VERSION: &str = "column from util-linux 2.40.2";

/// (short, long, takes an argument)
const OPTS: &[(char, &str, bool)] = &[
    ('t', "table", false),
    ('n', "table-name", true),
    ('O', "table-order", true),
    ('C', "table-column", true),
    ('N', "table-columns", true),
    ('l', "table-columns-limit", true),
    ('E', "table-noextreme", true),
    ('d', "table-noheadings", false),
    ('m', "table-maxout", false),
    ('e', "table-header-repeat", false),
    ('H', "table-hide", true),
    ('R', "table-right", true),
    ('T', "table-truncate", true),
    ('W', "table-wrap", true),
    ('L', "keep-empty-lines", false),
    ('J', "json", false),
    ('r', "tree", true),
    ('i', "tree-id", true),
    ('p', "tree-parent", true),
    ('c', "output-width", true),
    ('o', "output-separator", true),
    ('s', "separator", true),
    ('x', "fillrows", false),
    ('h', "help", false),
    ('V', "version", false),
];

/// Parsed command line: options in order with their arguments, and the operands.
pub struct Parsed {
    pub opts: Vec<(char, Option<String>)>,
    pub files: Vec<String>,
}

pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut opts = Vec::new();
    let mut files = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        i += 1;
        if a == "--" {
            files.extend(args[i..].iter().cloned());
            break;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (name, val) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_owned())),
                None => (long, None),
            };
            let exact = OPTS.iter().find(|o| o.1 == name);
            let cands: Vec<_> = OPTS.iter().filter(|o| o.1.starts_with(name)).collect();
            let o = match (exact, cands.len()) {
                (Some(o), _) => o,
                (None, 1) => cands[0],
                (None, 0) => return Err(format!("unknown option -- {name}")),
                _ => return Err(format!("ambiguous option -- {name}")),
            };
            if o.2 {
                let v = match val {
                    Some(v) => v,
                    None => {
                        let v = args.get(i).cloned();
                        i += 1;
                        v.ok_or_else(|| format!("option requires an argument -- {}", o.1))?
                    }
                };
                opts.push((o.0, Some(v)));
            } else if val.is_some() {
                return Err(format!("option doesn't take an argument -- {}", o.1));
            } else {
                opts.push((o.0, None));
            }
        } else if a.len() > 1 && a.starts_with('-') {
            let chars: Vec<char> = a[1..].chars().collect();
            let mut k = 0;
            while k < chars.len() {
                let c = chars[k];
                k += 1;
                let Some(o) = OPTS.iter().find(|o| o.0 == c) else {
                    return Err(format!("unknown option -- {c}"));
                };
                if o.2 {
                    let rest: String = chars[k..].iter().collect();
                    let v = if rest.is_empty() {
                        let v = args.get(i).cloned();
                        i += 1;
                        v.ok_or_else(|| format!("option requires an argument -- {c}"))?
                    } else {
                        rest
                    };
                    opts.push((c, Some(v)));
                    break;
                }
                opts.push((c, None));
            }
        } else {
            files.push(a.to_owned());
        }
    }
    Ok(Parsed { opts, files })
}
