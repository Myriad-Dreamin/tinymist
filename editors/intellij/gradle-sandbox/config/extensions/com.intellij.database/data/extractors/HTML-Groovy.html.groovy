/*
 * Available context bindings:
 *   COLUMNS     List<DataColumn>
 *   ROWS        Iterable<DataRow>
 *   OUT         { append() }
 *   FORMATTER   { format(row, col); formatValue(Object, col); getTypeName(Object, col); isStringLiteral(Object, col); }
 *   TRANSPOSED  Boolean
 * plus ALL_COLUMNS, TABLE, DIALECT
 *
 * where:
 *   DataRow     { rowNumber(); first(); last(); data(): List<Object>; value(column): Object }
 *   DataColumn  { columnNumber(), name() }
 */

import static com.intellij.openapi.util.text.StringUtil.escapeXmlEntities

NEWLINE = System.getProperty("line.separator")

def HTML_PATTERN = ~"<.+>"

def printRow = { values, tag, valueToString ->
  OUT.append("$NEWLINE<tr>$NEWLINE")
  values.each {
    def str = valueToString(it)
    def escaped = str ==~ HTML_PATTERN
      ? str
      : escapeXmlEntities((str as String).replaceAll("\\t|\\b|\\f", "")).replaceAll("\\r|\\n|\\r\\n", "<br/>")
    OUT.append("  <$tag>$escaped</$tag>$NEWLINE")
  }
  OUT.append("</tr>")
}

OUT.append(
"""<!DOCTYPE html>
<html>
  <head>
    <title></title>
    <meta charset="UTF-8">
  </head>
<body>
<table border="1" style="border-collapse:collapse">""")

if (!TRANSPOSED) {
  printRow(COLUMNS, "th") { it.name() }
  ROWS.each { row -> printRow(COLUMNS, "td") { FORMATTER.format(row, it) } }
}
else {
  def values = COLUMNS.collect { new ArrayList<String>( [it.name()] ) }
  ROWS.each { row -> COLUMNS.eachWithIndex { col, i -> values[i].add(FORMATTER.format(row, col)) } }
  values.each { printRow(it, "td", { it }) }
}

OUT.append("""
</table>
</body>
</html>
""")