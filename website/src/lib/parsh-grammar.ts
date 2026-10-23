/** The reader's structured comments nest; a bare semicolon elsewhere is text. */
export default {
  name: "parsh",
  displayName: "parsh",
  scopeName: "source.parsh",
  patterns: [
    { include: "#comment" },
    { include: "#string" },
    {
      begin: "(\\()(defn)\\s+([^\\s()\"'$]+)\\s+(\\()",
      beginCaptures: {
        1: { name: "punctuation.section.parens.parsh" },
        2: { name: "keyword.control.parsh" },
        3: { name: "entity.name.function.parsh" },
        4: { name: "punctuation.section.parens.parsh" },
      },
      end: "\\)",
      endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { name: "variable.parameter.parsh", match: "[^\\s()\"']+" },
      ],
    },
    {
      match: "(\\()(let|set|for|export)\\s+([^\\s()\"'$]+)",
      captures: {
        1: { name: "punctuation.section.parens.parsh" },
        2: { name: "keyword.control.parsh" },
        3: { name: "variable.other.parsh" },
      },
    },
    { name: "variable.other.parsh", match: "\\$[^\\s()\"']+" },
    {
      name: "keyword.control.parsh",
      match:
        "(?<=\\()(?:do|let|set|get|if|for|while|break|continue|defn|list|str|eq|not|glob|cd|export|exit)(?=[\\s)])",
    },
    {
      name: "keyword.operator.parsh",
      match: "(?<=\\()(?:[+*/=-]|!=|[<>]=?|\\|[12]?>?>?|<\\|)(?=[\\s)])",
    },
    { name: "entity.name.function.parsh", match: "(?<=\\()[^\\s()\"'$]+" },
    { name: "punctuation.section.parens.parsh", match: "[()]" },
  ],
  repository: {
    string: {
      patterns: [{ include: "#single-string" }, { include: "#double-string" }],
    },
    "single-string": {
      name: "string.quoted.single.parsh",
      begin: "'",
      end: "'",
    },
    "double-string": {
      name: "string.quoted.double.parsh",
      begin: '\"',
      end: '\"',
      patterns: [
        { name: "constant.character.escape.parsh", match: "\\\\." },
        { include: "#interpolation" },
        {
          name: "variable.other.parsh",
          match: "\\$[\\p{Alphabetic}_][\\p{Alphabetic}\\p{Number}_]*",
        },
      ],
    },
    interpolation: {
      begin: "\\$\\(",
      end: "\\)",
      beginCaptures: { 0: { name: "punctuation.section.interpolation.begin.parsh" } },
      endCaptures: { 0: { name: "punctuation.section.interpolation.end.parsh" } },
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { include: "#interpolation-list" },
        { include: "$self" },
      ],
    },
    "interpolation-list": {
      begin: "\\(",
      end: "\\)",
      beginCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      endCaptures: { 0: { name: "punctuation.section.parens.parsh" } },
      patterns: [
        { include: "#comment" },
        { include: "#string" },
        { include: "#interpolation-list" },
        { include: "$self" },
      ],
    },
    comment: {
      name: "comment.block.parsh",
      begin: "\\(\\s*;(?=[\\s()\"']|$)",
      end: "\\)",
      patterns: [{ include: "#string" }, { include: "#comment-body" }],
    },
    "comment-body": {
      begin: "\\(",
      end: "\\)",
      patterns: [{ include: "#string" }, { include: "#comment-body" }],
    },
  },
} as const;
