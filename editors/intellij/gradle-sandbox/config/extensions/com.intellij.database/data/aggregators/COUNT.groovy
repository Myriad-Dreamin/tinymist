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

def RES = 0G
ROWS.each { row ->
  COLUMNS.each { column ->
    RES += 1
  }
}
OUT.append(RES.toString())