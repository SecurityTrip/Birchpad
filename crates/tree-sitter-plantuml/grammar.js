/**
 * PlantUML for Birchpad: a grammar of lines, made for highlighting, folding and the Function
 * List, not for drawing. A diagram runs from its `@start` line to its `@end` line; blocks
 * (`alt` ... `end`, `box` ... `end box`, `!if` ... `!endif`, `!procedure` ... `!endprocedure`,
 * a line ending in `{` ... `}`) are nodes of their own, so that they fold; any other line is a
 * row of tokens.
 */

const BLOCKS = ['alt', 'opt', 'loop', 'par', 'par2', 'break', 'critical', 'group', 'box'];

const PARTICIPANTS = [
  'participant', 'actor', 'boundary', 'control', 'entity', 'database', 'collections', 'queue',
];

const KEYWORDS = [
  // Declarations of the other diagrams.
  'abstract', 'agent', 'annotation', 'archimate', 'artifact', 'card', 'circle', 'class',
  'cloud', 'component', 'diamond', 'enum', 'exception', 'file', 'folder', 'frame', 'hexagon',
  'interface', 'json', 'label', 'map', 'metaclass', 'node', 'object', 'package', 'person',
  'protocol', 'rectangle', 'stack', 'state', 'storage', 'struct', 'usecase',
  'namespace', 'together',
  // Sequence diagrams.
  'activate', 'deactivate', 'destroy', 'create', 'return', 'autonumber', 'newpage', 'ref',
  'over', 'of', 'as', 'also', 'else', 'end',
  // Activity diagrams.
  'start', 'stop', 'kill', 'detach', 'if', 'then', 'elseif', 'endif', 'while', 'endwhile',
  'repeat', 'backward', 'fork', 'again', 'merge', 'split', 'switch', 'case', 'endswitch',
  'partition', 'is', 'not',
  // Notes and the look of a diagram.
  'note', 'hnote', 'rnote', 'left', 'right', 'top', 'bottom', 'up', 'down', 'on', 'link',
  'legend', 'endlegend', 'endnote', 'header', 'endheader', 'footer', 'endfooter',
  'center', 'skinparam', 'skin', 'hide', 'show', 'remove', 'restore', 'scale', 'caption',
  'direction', 'extends', 'implements', 'allowmixing', 'mainframe',
];

module.exports = grammar({
  name: 'plantuml',

  extras: $ => [/[ \t\f\r]/, $.block_comment],

  externals: $ => [$._end_of_text, $._error_recovery],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.note, $.keyword],
  ],

  rules: {
    document: $ => repeat($._item),

    _item: $ => choice($.diagram, $._content),

    diagram: $ => prec.right(seq($.start_line, repeat($._content), optional($.end_line))),

    start_line: $ => seq(alias(/@start[A-Za-z_]*/, $.start), optional($.rest), $._line_end),
    end_line: $ => seq(alias(/@end[A-Za-z_]*/, $.end), optional($.rest), $._line_end),

    _content: $ => choice(
      $.block,
      $.if_block,
      $.procedure,
      $.brace_block,
      $.note,
      $.comment,
      $.include,
      $.directive,
      $.participant_declaration,
      $.section,
      $.title,
      $.statement,
      $._newline,
    ),

    // `alt` ... `else` ... `end`, `box` ... `end box`.
    block: $ => seq(
      field('opener', alias(choice(...BLOCKS), $.keyword)),
      optional($.rest),
      $._line_end,
      repeat($._content),
      $.block_end,
    ),
    // `end` or `end box` alone on its line: `end note` and its kin end other things.
    block_end: $ => alias(token(prec(3, /end([ \t]+box)?[ \t]*\n/)), $.keyword),

    // `!if` ... `!elseif` ... `!else` ... `!endif`.
    if_block: $ => seq(
      alias(token(prec(2, /!if(n?def)?/)), $.directive_name),
      optional($.rest),
      $._line_end,
      repeat(choice($._content, $.else_line)),
      $.endif_line,
    ),
    else_line: $ => seq(
      alias(token(prec(2, /!else(if)?/)), $.directive_name),
      optional($.rest),
      $._line_end,
    ),
    endif_line: $ => seq(
      alias(token(prec(2, '!endif')), $.directive_name),
      optional($.rest),
      $._line_end,
    ),

    // `!procedure $name($a)` ... `!endprocedure`, and functions.
    procedure: $ => seq(
      alias(token(prec(2, /!(unquoted[ \t]+)?(procedure|function|definelong)/)), $.directive_name),
      field('name', alias(/\$?[A-Za-z_][A-Za-z0-9_]*/, $.name)),
      optional($.rest),
      $._line_end,
      repeat($._content),
      alias(token(prec(2, /!end(procedure|function|definelong)/)), $.directive_name),
      $._line_end,
    ),

    // `package Shop {` ... `}`.
    brace_block: $ => seq(
      repeat1($._token),
      '{',
      $._line_end,
      repeat($._content),
      '}',
      optional($.rest),
      $._line_end,
    ),

    // `note left of A` ... `end note`, and legends, headers and footers: lines of text.
    note: $ => prec.dynamic(10, seq(
      alias(choice('note', 'hnote', 'rnote', 'legend', 'header', 'footer'), $.keyword),
      repeat($._plain_token),
      $._line_end,
      repeat(choice(alias(/[^\n]+/, $.text), $._newline)),
      alias(token(prec(4, /end[ \t]*(note|legend|header|footer)[^\n]*/)), $.keyword),
      $._line_end,
    )),

    comment: $ => seq(/'[^\n]*/, $._line_end),
    block_comment: $ => token(seq("/'", /([^']|'+[^'\/])*/, /'+\//)),

    // `!include path`, opened with Ctrl+click.
    include: $ => seq(
      alias(token(prec(3, /!(include(_many|_once|sub|url)?|import)/)), $.directive_name),
      optional(field('path', alias(/[^\s][^\n]*/, $.path))),
      $._line_end,
    ),

    // `!define`, `!theme`, `!pragma`, `!$name = value` and the other preprocessor lines.
    directive: $ => seq(
      alias(token(prec(1, /!\$?[A-Za-z_][A-Za-z0-9_]*/)), $.directive_name),
      repeat($._token),
      $._line_end,
    ),

    // `participant "Long name" as P #red`.
    participant_declaration: $ => seq(
      alias(choice(...PARTICIPANTS), $.keyword),
      field('name', choice($.string, $.identifier)),
      repeat($._token),
      $._line_end,
    ),

    // `== Section ==`.
    section: $ => seq(token(prec(2, /==+/)), field('name', alias(/[^=\s]([^=\n]*[^=\s])?/, $.name)), token(prec(2, /==+/)), $._line_end),

    title: $ => seq(
      alias('title', $.keyword),
      optional(field('name', alias(/[^\s][^\n]*/, $.name))),
      $._line_end,
    ),

    statement: $ => seq(repeat1($._token), $._line_end),

    _token: $ => choice($._plain_token, $.label),

    _plain_token: $ => choice(
      $.keyword,
      $.arrow,
      $.string,
      $.color,
      $.stereotype,
      $.number,
      $.variable,
      $.identifier,
      $.punctuation,
      seq('{', repeat($._token), '}'),
    ),

    keyword: $ => choice(...KEYWORDS),

    // `->`, `-->`, `<|--`, `*--`, `o..`, `-[#red]>`, `-up->`, `->x`.
    arrow: $ => token(prec(1, /[<*ox#}+|\\\/^]*(-+|\.+|==+)(\[[^\]\n]*\])?((up|down|left|right|u|d|l|r)(-+|\.+))?[>*ox#{+|\\\/^]*/)),

    string: $ => /"[^"\n]*"/,
    color: $ => /#[0-9A-Za-z]+([\\\/-][0-9A-Za-z]+)?/,
    stereotype: $ => /<<[^>\n]*>>/,
    number: $ => /\d+(\.\d+)?%?/,
    variable: $ => /\$[A-Za-z_][A-Za-z0-9_]*/,
    identifier: $ => /[A-Za-z_À-￿][A-Za-z0-9_.À-￿]*/,
    // What follows `:` on a line: a message, a label, an action.
    label: $ => /:[^\n]*/,
    punctuation: $ => /[()\[\],;=<>&|*+\/\\@?%~^!'`.-]/,

    rest: $ => /[^\n]+/,

    _newline: $ => /\n/,

    // A line ends at a line break, or where the text ends.
    _line_end: $ => choice($._newline, $._end_of_text),
  },
});
