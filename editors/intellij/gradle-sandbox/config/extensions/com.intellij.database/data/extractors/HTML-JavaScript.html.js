function eachWithIdx(iterable, f) { var i = iterable.iterator(); var idx = 0; while (i.hasNext()) f(i.next(), idx++); }
function mapEach(iterable, f) { var vs = []; eachWithIdx(iterable, function (i) { vs.push(f(i));}); return vs; }
function escape(str) {
  str = str.replace(/\t|\b|\f/g, "");
  str = com.intellij.openapi.util.text.StringUtil.escapeXml(str);
  str = str.replace(/\r|\n|\r\n/g, "<br/>");
  return str;
}
var isHTML = RegExp.prototype.test.bind(/^<.+>$/);

var NEWLINE = "\n";

function output() { for (var i = 0; i < arguments.length; i++) { OUT.append(arguments[i]); } }
function outputRow(items, tag) {
  output("<tr>");
  for (var i = 0; i < items.length; i++)
    output("<", tag, ">", isHTML(items[i]) ? items[i] : escape(items[i]), "</", tag, ">");
  output("</tr>", NEWLINE);
}


output("<!DOCTYPE html>", NEWLINE,
       "<html>", NEWLINE,
       "<head>", NEWLINE,
       "<title></title>", NEWLINE,
       "<meta charset=\"UTF-8\">", NEWLINE,
       "</head>", NEWLINE,
       "<body>", NEWLINE,
       "<table border=\"1\" style=\"border-collapse:collapse\">", NEWLINE);

if (TRANSPOSED) {
  var values = mapEach(COLUMNS, function(col) { return [col.name()]; });
  eachWithIdx(ROWS, function (row) {
    eachWithIdx(COLUMNS, function (col, i) {
      values[i].push(FORMATTER.format(row, col));
    });
  });
  eachWithIdx(COLUMNS, function (_, i) { outputRow(values[i], "td"); });
}
else {
  outputRow(mapEach(COLUMNS, function (col) { return col.name(); }), "th");
  eachWithIdx(ROWS, function (row) {
    outputRow(mapEach(COLUMNS, function (col) { return FORMATTER.format(row, col); }), "td")
  });
}

output("</table>", NEWLINE,
       "</body>", NEWLINE,
       "</html>", NEWLINE);