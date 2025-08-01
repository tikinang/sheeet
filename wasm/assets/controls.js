let rangeStartCell = null;
let rangeEndCell = null;
let selectedRange = new Set();
let isMouseDown = false;

function getCellCoordinates(cell) {
    const parts = cell.id.split("-");
    return {
        col: parseInt(parts[0]),
        row: parseInt(parts[1])
    };
}

function clearRangeSelection() {
    selectedRange.forEach(cellId => {
        const cell = document.getElementById(cellId);
        if (cell) {
            cell.classList.remove('selected-anchor', 'selected-top', 'selected-bottom', 'selected-left', 'selected-right');
        }
    });
    selectedRange.clear();

    rangeStartCell = null;
    rangeEndCell = null;
}

function forEachCell(startCell, endCell, handle) {
    const start = getCellCoordinates(startCell);
    const end = getCellCoordinates(endCell);

    const minCol = Math.min(start.col, end.col);
    const maxCol = Math.max(start.col, end.col);
    const minRow = Math.min(start.row, end.row);
    const maxRow = Math.max(start.row, end.row);

    for (let col = minCol; col <= maxCol; col++) {
        for (let row = minRow; row <= maxRow; row++) {
            handle(col, row, {minCol, maxCol, minRow, maxRow})
        }
    }
}

function selectRange(startCell, endCell) {
    clearRangeSelection();
    
    forEachCell(startCell, endCell, (col, row, boundaries) => {
        const cellId = `${col}-${row}`;
        const cell = document.getElementById(cellId);
        if (cell) {
            selectedRange.add(cellId);
            if (col === boundaries.minCol) {
                cell.classList.add('selected-left');
            }
            if (col === boundaries.maxCol) {
                cell.classList.add('selected-right');
            }
            if (row === boundaries.minRow) {
                cell.classList.add('selected-top');
            }
            if (row === boundaries.maxRow) {
                cell.classList.add('selected-bottom');
            }
        }
    })

    rangeStartCell = startCell
    rangeStartCell.classList.add('selected-anchor');

    rangeEndCell = endCell
    rangeEndCell.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
        inline: 'nearest'
    });
}

function selectCell(cell, extendRange = false) {
    if (!cell) {
        return;
    }

    if (extendRange && rangeStartCell) {
        rangeEndCell = cell;
        selectRange(rangeStartCell, cell);
    } else {
        selectRange(cell, cell)
    }
}


document.getElementById("cargo-toml-content").addEventListener('focus', function () {
    clearRangeSelection();
});
document.getElementById("lib-rs-content").addEventListener('focus', function () {
    clearRangeSelection();
});

document.getElementById("spreadsheet").addEventListener('focusout', async function (event) {
    event.preventDefault();
    event.target.textContent = window.wasmBindings.set_cell_raw_value(event.target.id, event.target.textContent.trim());
});


document.getElementById("spreadsheet").addEventListener('mousedown', function (event) {
    if (event.target.tagName === 'TD') {
        isMouseDown = true;
        selectCell(event.target, event.shiftKey);
        document.activeElement.blur()
        event.preventDefault();
    }
});

document.getElementById("spreadsheet").addEventListener('mousemove', function (event) {
    if (isMouseDown && event.target.tagName === 'TD') {
        selectCell(event.target, true);
        event.preventDefault();
    }
});

document.addEventListener('mouseup', function () {
    isMouseDown = false;
});

function addId(selectedId, vertical, val) {
    const parts = selectedId.split("-")
    if (vertical) {
        parts[1] = Number(parts[1]) + val
    } else {
        parts[0] = Number(parts[0]) + val
    }
    return `${parts[0]}-${parts[1]}`
}

let copiedRangeStartCell = null;
let copiedRangeEndCell = null;
let cut = false;

window.addEventListener('keydown', async function (event) {
    if (event.ctrlKey) {
        switch (event.key) {
            case 'Enter':
                event.preventDefault();
                saveCode(document.getElementById("cargo-toml-content"))
                saveCode(document.getElementById("lib-rs-content"))
                await window.compile();
                break;
            case 's':
                event.preventDefault();
                window.wasmBindings.save_app_state_to_local_storage()
                break;
            case 'c':
                event.preventDefault();
                copiedRangeStartCell = rangeStartCell
                copiedRangeEndCell = rangeEndCell
                break;
            case 'x':
                event.preventDefault();
                copiedRangeStartCell = rangeStartCell
                copiedRangeEndCell = rangeEndCell
                cut = true;
                break;
            case 'v':
                event.preventDefault();
                if (!copiedRangeStartCell || !copiedRangeEndCell) {
                    break;
                }
                const targetStartCell = getCellCoordinates(rangeStartCell);
                forEachCell(copiedRangeStartCell, copiedRangeEndCell, (col, row, boundaries) => {
                    const colDistance = targetStartCell.col - boundaries.minCol
                    const rowDistance = targetStartCell.row - boundaries.minRow
                    
                    const copiedCellId = `${col}-${row}`;
                    const targetCellId = `${col + colDistance}-${row + rowDistance}`
                    const newRawValue = window.wasmBindings.copy_cell_get_raw_value(copiedCellId, targetCellId);
                    document.getElementById(targetCellId).textContent = window.wasmBindings.set_cell_raw_value(targetCellId, newRawValue);

                    if (cut) {
                        const cutCell = document.getElementById(copiedCellId);
                        cutCell.textContent = window.wasmBindings.set_cell_raw_value(cutCell.id, '');
                        cut = false;
                    }
                })
                break;
        }
        return;
    }

    switch (event.key) {
        case 'Enter':
            if (!rangeStartCell) {
                break;
            }
            event.preventDefault();
            if (rangeStartCell.hasAttribute("contenteditable")) {
                rangeStartCell.removeAttribute("contenteditable");
            } else {
                if (rangeStartCell.textContent.trim()) {
                    rangeStartCell.textContent = window.wasmBindings.get_cell_raw_value(rangeStartCell.id)
                }
                rangeStartCell.setAttribute("contenteditable", "true");
                rangeStartCell.focus();
            }
            break;
        case 'Escape':
            if (!rangeStartCell) {
                break;
            }
            event.preventDefault();
            if (rangeStartCell.hasAttribute("contenteditable")) {
                rangeStartCell.removeAttribute("contenteditable");
            } else {
                clearRangeSelection();
            }
            break;
        case 'Delete':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            forEachCell(rangeStartCell, rangeEndCell, (col, row, _) => {
                const targetCellId = `${col}-${row}`;
                const toDeleteCell = document.getElementById(targetCellId);
                toDeleteCell.textContent = window.wasmBindings.set_cell_raw_value(toDeleteCell.id, '');                
            })
            break;
        case 'ArrowUp':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(rangeEndCell.id, true, -1)), event.shiftKey)
            break;
        case 'ArrowDown':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(rangeEndCell.id, true, 1)), event.shiftKey)
            break;
        case 'ArrowLeft':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(rangeEndCell.id, false, -1)), event.shiftKey)
            break;
        case 'ArrowRight':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(rangeEndCell.id, false, 1)), event.shiftKey)
            break;
        case 'Tab':
            if (!rangeStartCell || rangeStartCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            let val = 1
            if (event.shiftKey) {
                val = -1
            }
            selectCell(document.getElementById(addId(rangeEndCell.id, false, val)), event.shiftKey)
            break;
    }
});

function saveCode(element) {
    localStorage.setItem(element.id, element.textContent)
}
