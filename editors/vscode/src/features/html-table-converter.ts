import { DOMParser } from '@xmldom/xmldom';

export async function convertHtmlToTypst(html: string): Promise<string> {
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    
    const table = doc.getElementsByTagName('table')[0];
    if (!table) {
        // actually it won't cause an error
        throw new Error('Table element not found. Please check the HTML content.');
    }

    const rows = table.getElementsByTagName('tr');
    if (rows.length === 0) {
        throw new Error('No tr tag found in the table.');
    }

    const firstRow = rows[0];
    const firstRowCells = Array.from(firstRow.getElementsByTagName('th')).concat(
        Array.from(firstRow.getElementsByTagName('td'))
    );
    
    const columnCount = firstRowCells.reduce((count, cell) => {
        const colspanAttr = cell.getAttribute('colspan');
        return count + (colspanAttr ? Number(colspanAttr) : 1);
    }, 0);

    let out = `#table(\n  columns: ${columnCount},\n`;

    for (let i = 0; i < rows.length; i++) {
        out += '  ';
        const row = rows[i];
        const cells = Array.from(row.getElementsByTagName('th')).concat(
            Array.from(row.getElementsByTagName('td'))
        );
        
        for (let j = 0; j < cells.length; j++) {
            const cell = cells[j];
            const rowspan = cell.getAttribute('rowspan') ? Number(cell.getAttribute('rowspan')) : 1;
            const colspan = cell.getAttribute('colspan') ? Number(cell.getAttribute('colspan')) : 1;
            const spanOpts =
                `${rowspan > 1 ? `rowspan: ${rowspan}, ` : ''}${colspan > 1 ? `colspan: ${colspan}, ` : ''}`;
            const content = cell.textContent?.trim() || '';
            out += spanOpts ? `table.cell(${spanOpts})[${content}], ` : `[${content}], `;
        }
        out += '\n';
    }

    return out + ')';
}
