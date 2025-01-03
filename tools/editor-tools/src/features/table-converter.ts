import { JSDOM } from 'jsdom';

export function convertHtmlToTypst(html: string): string {
    const dom = new JSDOM(html);
    const document = dom.window.document;

    const table = document.querySelector('table');
    if (!table) {
        throw new Error('No table found in clipboard content');
    }

    const rows = table.rows;
    const firstRow = rows[0];
    const columnCount = Array.from(firstRow.cells).reduce((total, cell) => {
        return total + (parseInt(cell.getAttribute('colspan') || '1'));
    }, 0);

    let typstOutput = `#table(columns: ${columnCount},\n`;

    for (let i = 0; i < rows.length; i++) {
        const cells = rows[i].cells;
        typstOutput += '  ';
        
        for (let j = 0; j < cells.length; j++) {
            const cell = cells[j];
            const rowspan = cell.getAttribute('rowspan');
            const colspan = cell.getAttribute('colspan');
            
            if (rowspan || colspan) {
                typstOutput += `table.cell(${rowspan ? `rowspan: ${rowspan}, ` : ''}${
                    colspan ? `colspan: ${colspan}, ` : ''
                })[${cell.textContent?.trim() || ''}], `;
            } else {
                typstOutput += `[${cell.textContent?.trim() || ''}], `;
            }
        }
        typstOutput += '\n';
    }

    typstOutput += ')';
    return typstOutput;
}
