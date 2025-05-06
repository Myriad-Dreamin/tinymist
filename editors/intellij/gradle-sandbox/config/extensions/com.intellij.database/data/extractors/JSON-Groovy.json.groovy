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


import static com.intellij.openapi.util.text.StringUtil.escapeStringCharacters as escapeStr

NEWLINE = System.getProperty("line.separator")
INDENT = "  "

def printJSON(level, col, o) {
  switch (o) {
    case null: OUT.append("null"); break
    case Tuple: printJSON(level, o[0], o[1]); break
    case Map:
      OUT.append("{")
      o.entrySet().eachWithIndex { entry, i ->
        OUT.append("${i > 0 ? "," : ""}$NEWLINE${INDENT * (level + 1)}")
        OUT.append("\"${escapeStr(entry.getKey().toString())}\"")
        OUT.append(": ")
        printJSON(level + 1, col, entry.getValue())
      }
      OUT.append("$NEWLINE${INDENT * level}}")
      break
    case Object[]:
    case Iterable:
      OUT.append("[")
      def plain = true
      o.eachWithIndex { item, i ->
        plain = item == null || item instanceof Number || item instanceof Boolean || item instanceof String
        if (plain) {
          OUT.append(i > 0 ? ", " : "")
        }
        else {
          OUT.append("${i > 0 ? "," : ""}$NEWLINE${INDENT * (level + 1)}")
        }
        printJSON(level + 1, col, item)
      }
      if (plain) OUT.append("]") else OUT.append("$NEWLINE${INDENT * level}]")
      break
    case Boolean: OUT.append("$o"); break
    default:
      def str = FORMATTER.formatValue(o, col)
      def typeName = FORMATTER.getTypeName(o, col)
      def shouldQuote = FORMATTER.isStringLiteral(o, col) && !(typeName.equalsIgnoreCase("json") || typeName.equalsIgnoreCase("jsonb"))
      OUT.append(shouldQuote ? "\"${escapeStr(str)}\"" : str);
      break
  }
}

printJSON(0, null, ROWS.transform { row ->
  def map = new LinkedHashMap<String, Object>()
  COLUMNS.each { col ->
    if (row.hasValue(col)) {
      def val = row.value(col)
      map.put(col.name(), new Tuple(col, val))
    }
  }
  map
})