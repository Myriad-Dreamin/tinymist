import * as cheerio from 'cheerio';

export async function convertHtmlToTypst(html: string): Promise<string> {
    const $ = cheerio.load(html);
    const table = $('table').first();
    if (!table.length) {
        throw new Error('Table element not found. Please check the HTML content.');
    }

    const rows = table.find('tr');
    if (!rows.length) {
        throw new Error('No tr tag found in the table.');
    }

    const firstRow = rows.first();
    const firstRowCells = firstRow.find('td, th');
    const columnCount = firstRowCells.toArray().reduce((count, cell) => {
        const colspanAttr = $(cell).attr('colspan');
        return count + (Number(colspanAttr) || 1);
    }, 0);

    let out = `#table(\n  columns: ${columnCount},\n`;

    rows.each((_, rowElem) => {
        out += '  ';
        const row = $(rowElem);
        const cells = row.find('td, th');
        cells.each((_, cellElem) => {
            const cell = $(cellElem);
            const rowspan = Number(cell.attr('rowspan')) || 1;
            const colspan = Number(cell.attr('colspan')) || 1;
            const spanOpts =
                `${rowspan > 1 ? `rowspan: ${rowspan}, ` : ''}${colspan > 1 ? `colspan: ${colspan}, ` : ''}`;
            const content = cell.text().trim();
            out += spanOpts ? `table.cell(${spanOpts})[${content}], ` : `[${content}], `;
        });
        out += '\n';
    });

    return out + ')';
}
