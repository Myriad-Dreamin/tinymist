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

BigDecimal RES = 0
int i = 0
ROWS.each { row ->
  COLUMNS.each { column ->
    def value = row.value(column)
    if (value instanceof Number) {
      RES = RES.add(value, DECIMAL128)
      i++
    }
    else if (value.toString().isBigDecimal()) {
      RES = RES.add(value.toString().toBigDecimal(), DECIMAL128)
      i++
    }
  }
}
if (i > 0) {
  RES = RES.divide(i, DECIMAL128)
  OUT.append(RES.toString())
}
else {
  OUT.append("Not enough values")
}