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

SEPARATOR = ", "
QUOTE     = "'"
STRING_PREFIX = DIALECT.getDbms().isMicrosoft() ? "N" : ""
KEYWORDS_LOWERCASE = com.intellij.database.util.DbSqlUtil.areKeywordsLowerCase(PROJECT)
KW_NULL = KEYWORDS_LOWERCASE ? "null" : "NULL"

first = true
ROWS.each { row ->
    COLUMNS.each { column ->
        def value = row.value(column)
        def stringValue = value == null ? KW_NULL : FORMATTER.formatValue(value, column)
        def isStringLiteral = value != null && FORMATTER.isStringLiteral(value, column)
        if (isStringLiteral && DIALECT.getDbms().isMysql()) stringValue = stringValue.replace("\\", "\\\\")
        OUT.append(first ? "" : SEPARATOR)
                .append(isStringLiteral ? (STRING_PREFIX + QUOTE) : "")
                .append(stringValue ? stringValue.replace(QUOTE, QUOTE + QUOTE) : stringValue)
                .append(isStringLiteral ? QUOTE : "")
        first = false
    }
}