use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{self, Write},
};

#[derive(Parser)]
#[command(
    name = "skills",
    bin_name = "parsh skills",
    about = "Discover and load knowledge bundled with this version of parsh",
    after_help = "With no command, list available topics."
)]
struct SkillsCli {
    #[command(subcommand)]
    command: Option<SkillsCommand>,

    /// Write structured JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum SkillsCommand {
    /// List available topics and their descriptions
    List,
    /// Print Markdown guides for one or more topics
    Get {
        #[arg(value_name = "TOPIC", required_unless_present = "all")]
        topics: Vec<String>,

        /// Print every topic in the catalog
        #[arg(long, conflicts_with = "topics")]
        all: bool,

        /// Include related example scripts where available
        #[arg(long)]
        full: bool,
    },
    /// Print the logical resource path for a topic or the catalog
    Path {
        #[arg(value_name = "TOPIC")]
        topic: Option<String>,
    },
}

struct Topic {
    name: &'static str,
    description: &'static str,
    content: &'static str,
    examples: &'static [Example],
}

struct Example {
    path: &'static str,
    content: &'static str,
}

// Cargo packages the linked docs and scripts as files, so these paths work
// in the repository and when building the packaged crate.
macro_rules! topic {
    ($name:literal, $description:literal $(, $example:literal)* $(,)?) => {
        Topic {
            name: $name,
            description: $description,
            content: include_str!(concat!("../skill-data/", $name, ".md")),
            examples: &[$(Example {
                path: concat!("examples/", $example),
                content: include_str!(concat!("../skill-data/examples/", $example)),
            }),*],
        }
    };
}

const TOPICS: &[Topic] = &[
    topic!(
        "commands",
        "Command lookup, aliases, pipelines, capture, and redirection.",
        "pipelines.parsh",
        "files.parsh",
    ),
    topic!(
        "control-flow",
        "Functions, branching, sequencing, and iteration.",
        "functions.parsh",
        "for.parsh",
        "while.parsh",
    ),
    topic!(
        "core",
        "Start here for a syntax primer and topic selection."
    ),
    topic!(
        "design",
        "The language and execution principles behind parsh."
    ),
    topic!(
        "development",
        "Repository checks, website development, and WASM builds."
    ),
    topic!(
        "diagnostics",
        "Diagnostics, command failures, errors, and exit codes."
    ),
    topic!(
        "examples",
        "Runnable example scripts organized by language feature.",
        "arithmetic.parsh",
        "bytes.parsh",
        "execution.parsh",
        "files.parsh",
        "for.parsh",
        "functional.parsh",
        "functions.parsh",
        "hello.parsh",
        "http.parsh",
        "json.parsh",
        "lib/greetings.parsh",
        "lib/numbers.parsh",
        "literals.parsh",
        "loading.parsh",
        "macros.parsh",
        "maps.parsh",
        "metaprogramming.parsh",
        "pipelines.parsh",
        "stdlib.parsh",
        "structs.parsh",
        "tasks.parsh",
        "variables.parsh",
        "while.parsh",
    ),
    topic!(
        "execution",
        "Execution results, capture, exceptions, and tasks.",
        "execution.parsh",
        "tasks.parsh",
    ),
    topic!(
        "getting-started",
        "Installation, CLI modes, and startup configuration.",
        "hello.parsh",
    ),
    topic!(
        "interactive",
        "Interactive editing, history, completion, and sessions."
    ),
    topic!(
        "lisp",
        "Function composition, macros, forms, evaluation, and source loading.",
        "functional.parsh",
        "metaprogramming.parsh",
        "loading.parsh",
        "lib/greetings.parsh",
        "lib/numbers.parsh",
    ),
    topic!("playground", "Browser playground capabilities and limits."),
    topic!(
        "stdlib",
        "Standard library namespaces, data, JSON, bytes, and HTTP.",
        "stdlib.parsh",
        "json.parsh",
        "bytes.parsh",
        "http.parsh",
    ),
    topic!(
        "structs-and-macros",
        "Struct definitions, constructors, macros, and expansion.",
        "structs.parsh",
        "macros.parsh",
    ),
    topic!(
        "syntax",
        "S-expressions, literals, values, and string interpolation.",
        "literals.parsh",
        "arithmetic.parsh",
    ),
    topic!(
        "variables",
        "Variables, lexical scope, maps, interpolation, and arguments.",
        "variables.parsh",
        "maps.parsh",
    ),
];

const CATALOG_PATH: &str = "embedded://parsh/skills";

pub(super) fn run(args: impl Iterator<Item = OsString>) -> io::Result<i32> {
    let args: Vec<_> = args.collect();
    let mut output = io::stdout().lock();
    let cli = match SkillsCli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => {
            let json = args
                .iter()
                .skip(1)
                .take_while(|arg| *arg != "--")
                .any(|arg| arg == "--json");
            if json && error.use_stderr() {
                write_error(&mut output, true, error.to_string().trim_end())?;
            } else {
                error.print()?;
            }
            return Ok(error.exit_code());
        }
    };

    match cli.command.unwrap_or(SkillsCommand::List) {
        SkillsCommand::List => write_list(&mut output, cli.json)?,
        SkillsCommand::Get { topics, all, full } => match select_topics(&topics, all) {
            Ok(topics) => write_guides(&mut output, &topics, full, cli.json)?,
            Err(name) => return unknown_topic(&mut output, name, cli.json),
        },
        SkillsCommand::Path { topic } => {
            if let Some(name) = topic {
                if find_topic(&name).is_none() {
                    return unknown_topic(&mut output, &name, cli.json);
                }
                let path = format!("{CATALOG_PATH}/{name}");
                if cli.json {
                    write_json(&mut output, json!({"name": name, "path": path}))?;
                } else {
                    writeln!(output, "{path}")?;
                }
            } else if cli.json {
                write_json(&mut output, json!({"paths": [CATALOG_PATH]}))?;
            } else {
                writeln!(output, "{CATALOG_PATH}")?;
            }
        }
    }
    Ok(0)
}

fn find_topic(name: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|topic| topic.name == name)
}

fn select_topics(names: &[String], all: bool) -> Result<Vec<&'static Topic>, &str> {
    if all {
        return Ok(TOPICS.iter().collect());
    }
    names
        .iter()
        .map(|name| find_topic(name).ok_or(name.as_str()))
        .collect()
}

fn write_list(output: &mut impl Write, json: bool) -> io::Result<()> {
    if json {
        let topics: Vec<_> = TOPICS
            .iter()
            .map(|topic| json!({"name": topic.name, "description": topic.description}))
            .collect();
        return write_json(output, json!(topics));
    }

    let width = TOPICS
        .iter()
        .map(|topic| topic.name.len())
        .max()
        .unwrap_or(0);
    for topic in TOPICS {
        writeln!(output, "  {:<width$}  {}", topic.name, topic.description)?;
    }
    Ok(())
}

fn write_guides(
    output: &mut impl Write,
    topics: &[&Topic],
    full: bool,
    json: bool,
) -> io::Result<()> {
    if json {
        let guides: Vec<_> = topics
            .iter()
            .map(|topic| {
                let mut guide = json!({"name": topic.name, "content": topic.content});
                if full && !topic.examples.is_empty() {
                    let files: Vec<_> = topic
                        .examples
                        .iter()
                        .map(|example| json!({"path": example.path, "content": example.content}))
                        .collect();
                    guide["files"] = json!(files);
                }
                guide
            })
            .collect();
        return write_json(output, json!(guides));
    }

    for (index, topic) in topics.iter().enumerate() {
        if index > 0 {
            writeln!(output, "\n---\n")?;
        }
        writeln!(output, "{}", topic.content.trim_end_matches('\n'))?;
        if full {
            for example in topic.examples {
                writeln!(
                    output,
                    "\n### {}\n\n~~~parsh\n{}\n~~~",
                    example.path,
                    example.content.trim_end_matches('\n')
                )?;
            }
        }
    }
    Ok(())
}

fn write_json(output: &mut impl Write, data: Value) -> io::Result<()> {
    writeln!(output, "{}", json!({"success": true, "data": data}))
}

fn write_error(output: &mut impl Write, json: bool, message: &str) -> io::Result<()> {
    if json {
        writeln!(output, "{}", json!({"success": false, "error": message}))
    } else {
        writeln!(io::stderr().lock(), "parsh skills: {message}")
    }
}

fn unknown_topic(output: &mut impl Write, name: &str, json: bool) -> io::Result<i32> {
    write_error(
        output,
        json,
        &format!("unknown topic {name:?}. Run 'parsh skills list' for available topics."),
    )?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sorted_and_selects_topics_in_request_order() {
        assert!(TOPICS.windows(2).all(|pair| pair[0].name < pair[1].name));
        assert!(find_topic("core").is_some());

        let names = ["syntax".to_owned(), "commands".to_owned()];
        let names_in = |topics: Vec<&Topic>| {
            topics
                .into_iter()
                .map(|topic| topic.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names_in(select_topics(&names, false).unwrap()),
            ["syntax", "commands"]
        );
        assert_eq!(
            names_in(select_topics(&[], true).unwrap()),
            TOPICS.iter().map(|topic| topic.name).collect::<Vec<_>>()
        );
        assert!(matches!(
            select_topics(&["missing".to_owned()], false),
            Err("missing")
        ));
    }

    #[test]
    fn full_json_includes_example_sources() {
        let topic = find_topic("syntax").unwrap();
        let mut output = Vec::new();
        write_guides(&mut output, &[topic], true, true).unwrap();
        let guide: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            guide,
            json!({
                "success": true,
                "data": [{
                    "name": topic.name,
                    "content": topic.content,
                    "files": [
                        {"path": topic.examples[0].path, "content": topic.examples[0].content},
                        {"path": topic.examples[1].path, "content": topic.examples[1].content},
                    ],
                }]
            })
        );
    }
}
