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

def sum = BigDecimal.ZERO
values.forEach { value ->
  sum = sum.add(value, DECIMAL128)
}
def avg = sum.divide(values.size(), DECIMAL128)
def sumSquaredDiff = BigDecimal.ZERO
values.each { value ->
  BigDecimal diff = value.subtract(avg, DECIMAL128)
  sumSquaredDiff = sumSquaredDiff.add(diff.multiply(diff, DECIMAL128), DECIMAL128)
}

def variance = sumSquaredDiff.divide(values.size(), DECIMAL128)
def standardDeviation = variance.sqrt(DECIMAL128)
def cv = standardDeviation.divide(avg, DECIMAL128)
OUT.append((cv * 100).round(2) + "%")
