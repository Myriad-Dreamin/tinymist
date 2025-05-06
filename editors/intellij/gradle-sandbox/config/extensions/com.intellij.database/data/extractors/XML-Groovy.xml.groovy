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


import com.intellij.openapi.util.text.StringUtil

import java.util.regex.Pattern

NEWLINE = System.getProperty("line.separator")

pattern = Pattern.compile("[^\\w\\d]")
def escapeTag(name) {
  name = pattern.matcher(name).replaceAll("_")
  return name.isEmpty() || !Character.isLetter(name.charAt(0)) ? "_$name" : name
}
def printRow(level, rowTag, values) {
  def prefix = "$NEWLINE${StringUtil.repeat("  ", level)}"
  OUT.append("$prefix<$rowTag>")
  values.each { name, col, valuesName, value ->
    switch (value) {
      case Map:
        def mapValues = new ArrayList<Tuple>()
        value.each { key, v -> mapValues.add(new Tuple(escapeTag(key.toString()), col, key.toString(), v)) }
        printRow(level + 1, name, mapValues)
        break
      case Object[]:
      case Iterable:
        def listItems = new ArrayList<Tuple>()
        def itemName = valuesName != null ? escapeTag(StringUtil.unpluralize(valuesName) ?: "item") : "item"
        value.collect { v -> listItems.add(new Tuple(itemName, col, null, v)) }
        printRow(level + 1, name, listItems)
        break
      default:
        OUT.append("$prefix  <$name>")
        if (value == null) OUT.append("null")
        else {
          def formattedValue = FORMATTER.formatValue(value, col)
          if (isXmlString(formattedValue)) OUT.append(formattedValue)
          else OUT.append(StringUtil.escapeXmlEntities(formattedValue))
        }
        OUT.append("</$name>")
    }
  }
  OUT.append("$prefix</$rowTag>")
}

def isXmlString(string) {
  return string.startsWith("<") && string.endsWith(">") && (string.contains("</") || string.contains("/>"))
}

OUT.append(
"""<?xml version="1.0" encoding="UTF-8"?>
<data>""")

if (!TRANSPOSED) {
  ROWS.each { row ->
    def values = COLUMNS
      .findAll { col -> row.hasValue(col) }
      .collect { col ->
        new Tuple(escapeTag(col.name()), col, col.name(), row.value(col))
      }
    printRow(0, "row", values)
  }
}
else {
  def values = COLUMNS.collect { new ArrayList<Tuple>() }
  ROWS.eachWithIndex { row, rowIdx ->
    COLUMNS.eachWithIndex { col, colIdx ->
      if (row.hasValue(col)) {
        def value = row.value(col)
        values[colIdx].add(new Tuple("row${rowIdx + 1}", col, col.name(), value))
      }
    }
  }
  values.eachWithIndex { it, index ->
    printRow(0, escapeTag(COLUMNS[index].name()), it)
  }
}

OUT.append("""
</data>
""")