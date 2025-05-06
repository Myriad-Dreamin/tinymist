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

import static java.math.MathContext.DECIMAL128

def toBigDecimal = { value ->
  value instanceof Number ? value as BigDecimal :
  value.toString().isBigDecimal() ? value.toString() as BigDecimal :
  null
}

def values = []

ROWS.each { row ->
  COLUMNS.each { column ->
    def bigDecimal = toBigDecimal(row.value(column))
    if (bigDecimal != null) {
      values.add(bigDecimal)
    }
  }
}

if (values.isEmpty()) {
  OUT.append("Not enough values")
  return
}
elementsNumber = values.size()
Collections.sort(values)
mid = (int)elementsNumber / 2
RES = elementsNumber % 2 != 0 ? values[mid] : values[mid].add(values[mid - 1], DECIMAL128).divide(2, DECIMAL128)
OUT.append(RES.toString())