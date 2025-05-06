package extensions.data.extractors

NEWLINE = System.getProperty("line.separator")
SEPARATOR = "|"
BACKSLASH = "\\"
BACKQUOTE = "`"
LTAG = "<"
RTAG = ">"
ASTERISK = "*"
UNDERSCORE = "_"
LPARENTH = "("
RPARENTH = ")"
LBRACKET = "["
RBRACKET = "]"
TILDE = "~"

def printRow = { values, firstBold = false, valueToString ->
  values.eachWithIndex { value, idx ->
    def str = valueToString(value)
      .replace(BACKSLASH, BACKSLASH + BACKSLASH)
      .replace(SEPARATOR, BACKSLASH + SEPARATOR)
      .replace(BACKQUOTE, BACKSLASH + BACKQUOTE)
      .replace(ASTERISK, BACKSLASH + ASTERISK)
      .replace(UNDERSCORE, BACKSLASH + UNDERSCORE)
      .replace(LPARENTH, BACKSLASH + LPARENTH)
      .replace(RPARENTH, BACKSLASH + RPARENTH)
      .replace(LBRACKET, BACKSLASH + LBRACKET)
      .replace(RBRACKET, BACKSLASH + RBRACKET)
      .replace(TILDE, BACKSLASH + TILDE)
      .replace(LTAG, "&lt;")
      .replace(RTAG, "&gt;")
      .replaceAll("\r\n|\r|\n", "<br/>")
      .replaceAll("\t|\b|\f", "")

    OUT.append("| ")
      .append(firstBold && idx == 0 ? "**" : "")
      .append(str)
      .append(firstBold && idx == 0 ? "**" : "")
      .append(idx != values.size() - 1 ? " " : " |" + NEWLINE)
  }
}

if (TRANSPOSED) {
  def values = COLUMNS.collect { new ArrayList<String>([it.name()]) }
  def rowCount = 0
  ROWS.forEach { row ->
    COLUMNS.eachWithIndex { col, i -> values[i].add(FORMATTER.format(row, col)) }
    rowCount++
  }
  for (int i = 0; i <= rowCount; i++) {
    OUT.append("| ")
  }
  OUT.append("|" + NEWLINE)
  for (int i = 0; i <= rowCount; i++) {
    OUT.append("| :- ")
  }
  OUT.append("|" + NEWLINE)
  values.each { printRow(it, true) { it } }
}
else {
  printRow(COLUMNS) { it.name() }
  COLUMNS.each { OUT.append("| :--- ") }
  OUT.append("|" + NEWLINE)
  ROWS.each { row -> printRow(COLUMNS) { FORMATTER.format(row, it) } }
}