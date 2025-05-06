import com.intellij.openapi.util.text.StringUtil

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


WIDTH_BASED_ON_CONTENT = -1

PIPE = "|"
SPACE = " "
CROSS = "+"
MINUS = "-"
ROW_SEPARATORS = false
COLUMN_WIDTH = WIDTH_BASED_ON_CONTENT
NEWLINE = System.getProperty("line.separator")

static def splitByLines(values, size) {
  def splitValues = new ArrayList<>()
  def maxLines = 0
  for (int i = 0; i < size; i++) {
    def splitValue = StringUtil.splitByLines(values(i))
    splitValues.add(splitValue)
    maxLines = Math.max(maxLines, splitValue.size())
  }

  def byLines = new ArrayList<>(maxLines)
  for (int i = 0; i < maxLines; i++) {
    def lineValues = new ArrayList<>()
    byLines.add(lineValues)
    for (int j = 0; j < splitValues.size(); j++) {
      def splitValue = splitValues[j]
      lineValues.add(splitValue.size() <= i ? null : splitValue[i])
    }
  }
  return byLines
}

def printRow(values, size, width = { COLUMN_WIDTH }, padding = SPACE, separator = { PIPE }) {
  def byLines = splitByLines(values, size)
  byLines.each { line ->
    def lineSize = line.size()
    if (lineSize > 0) OUT.append(separator(-1))
    for (int i = 0; i < lineSize; i++) {
      def value = line[i] == null ? "" : line.get(i)
      def curWidth = width(i)
      OUT.append(value.padRight(curWidth, padding))
      OUT.append(separator(i))
    }
    OUT.append(NEWLINE)
  }
}

def printRows() {
  def colNames = COLUMNS.collect { it.name() }
  def calcWidth = COLUMN_WIDTH == WIDTH_BASED_ON_CONTENT
  def rows
  def width
  def rowFormatter
  if (calcWidth) {
    rows = new ArrayList<>()
    def widths = new int[COLUMNS.size()]
    COLUMNS.eachWithIndex { column, idx -> widths[idx] = column.name().length() }
    ROWS.each { row ->
      def rowValues = COLUMNS.withIndex().collect { col, idx ->
        def value = FORMATTER.format(row, col)
        widths[idx] = Math.max(widths[idx], value.length())
        value
      }
      rows.add(rowValues)
    }
    width = { widths[it] }
    rowFormatter = { it }
  }
  else {
    rows = ROWS
    width = { COLUMN_WIDTH }
    rowFormatter = { COLUMNS.collect { col -> FORMATTER.format(it, col) } }
  }

  printRow({""}, COLUMNS.size(), { width(it) }, MINUS) { CROSS }
  printRow( { colNames[it] }, COLUMNS.size()) { width(it) }

  def first = true
  rows.each { row ->
    def rowValues = rowFormatter(row)
    if (first || ROW_SEPARATORS) printRow({""}, COLUMNS.size(), { width(it) }, MINUS) { first ? CROSS : MINUS }
    printRow({ rowValues[it] }, rowValues.size()) { width(it) }
    first = false
  }
  printRow({""}, COLUMNS.size(), { width(it) }, MINUS) { CROSS }
}

def printRowsTransposed() {
  def calcWidth = COLUMN_WIDTH == WIDTH_BASED_ON_CONTENT
  if (calcWidth) {
    COLUMN_WIDTHS = new ArrayList<Integer>()
    COLUMN_WIDTHS.add(0)
  }
  def valuesByRow = COLUMNS.collect { col ->
    if (calcWidth) COLUMN_WIDTHS.set(0, Math.max(COLUMN_WIDTHS[0], col.name().length()))
    new ArrayList<String>([col.name()])
  }
  def rowCount = 1
  ROWS.each { row ->
    rowCount++
    COLUMNS.eachWithIndex { col, i ->
      def formattedValue = FORMATTER.format(row, col)
      valuesByRow[i].add(formattedValue)
      def widthIdx = rowCount - 1
      def length = formattedValue.length()
      if (calcWidth) {
        if (COLUMN_WIDTHS.size() == widthIdx) COLUMN_WIDTHS.add(length)
        COLUMN_WIDTHS.set(widthIdx, Math.max(COLUMN_WIDTHS[widthIdx], length))
      }
    }
  }
  valuesByRow.each { row ->
    printRow({ "" }, rowCount, { calcWidth ? COLUMN_WIDTHS[it] : COLUMN_WIDTH }, MINUS) {
      it <= 0 ? CROSS : it == rowCount - 1 ? CROSS : MINUS
    }
    printRow({ row[it] }, row.size()) { calcWidth ? COLUMN_WIDTHS[it] : COLUMN_WIDTH }
  }
  printRow({ "" }, rowCount, { calcWidth ? COLUMN_WIDTHS[it] : COLUMN_WIDTH }, MINUS) {
    it <= 0 ? CROSS : it == rowCount - 1 ? CROSS : MINUS
  }
}

if (TRANSPOSED) {
  printRowsTransposed()
}
else {
  printRows()
}
