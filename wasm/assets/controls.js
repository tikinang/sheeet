document.getElementById("spreadsheet").addEventListener('focusout', async function (event) {
    event.preventDefault();
    console.debug("in focusout");
    event.target.textContent = window.wasmBindings.set_cell_raw_value(event.target.id, event.target.textContent.trim());
});

let selectedCell;
let copiedCellId;
let cutCellContent;

function selectCell(cell) {
    if (!cell) {
        return;
    }
    if (selectedCell) {
        selectedCell.classList.remove("selected");
        if (selectedCell.hasAttribute("contenteditable")) {
            selectedCell.removeAttribute("contenteditable");
        }
    }
    selectedCell = cell
    selectedCell.classList.add("selected");
    selectedCell.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
        inline: 'nearest'
    });
}

document.getElementById("spreadsheet").addEventListener('click', async function (event) {
    selectCell(event.target)
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

window.addEventListener('keydown', async function (event) {
    if (event.ctrlKey) {
        switch (event.key) {
            case 'Enter':
                event.preventDefault();
                saveCode(document.getElementById("cargo-toml-content"))
                saveCode(document.getElementById("lib-rs-content"))
                await compile();
                break;
            case 's':
                event.preventDefault();
                window.wasmBindings.save_app_state_to_local_storage()
                break;
            case 'c':
                event.preventDefault();
                copiedCellId = selectedCell.id
                break;
            case 'x':
                event.preventDefault();
                cutCellContent = selectedCell.textContent
                selectedCell.textContent = window.wasmBindings.set_cell_raw_value(selectedCell.id, '');
                break;
            case 'v':
                event.preventDefault();
                if (cutCellContent) {
                    console.log("paste cut cell:", selectedCell.id, cutCellContent);
                    selectedCell.textContent = window.wasmBindings.set_cell_raw_value(selectedCell.id, cutCellContent);
                    cutCellContent = null;
                } else if (copiedCellId) {
                    console.log("paste copied cell:", selectedCell.id, copiedCellId);
                    const newRawValue = window.wasmBindings.copy_cell_get_raw_value(copiedCellId, selectedCell.id);
                    selectedCell.textContent = window.wasmBindings.set_cell_raw_value(selectedCell.id, newRawValue);
                }
                break;
        }
        return;
    }

    switch (event.key) {
        case 'Enter':
            if (!selectedCell) {
                break;
            }
            event.preventDefault();
            console.debug("in enter")
            if (selectedCell.hasAttribute("contenteditable")) {
                selectedCell.removeAttribute("contenteditable");
            } else {
                if (selectedCell.textContent.trim()) {
                    selectedCell.textContent = window.wasmBindings.get_cell_raw_value(selectedCell.id)
                }
                selectedCell.setAttribute("contenteditable", "true");
                selectedCell.focus();
            }
            break;
        case 'Escape':
            if (!selectedCell) {
                break;
            }
            event.preventDefault();
            if (selectedCell.hasAttribute("contenteditable")) {
                selectedCell.removeAttribute("contenteditable");
            } else {
                selectedCell.classList.remove("selected");
                selectedCell = null
            }
            break;
        case 'ArrowUp':
            if (!selectedCell || selectedCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(selectedCell.id, true, -1)))
            break;
        case 'ArrowDown':
            if (!selectedCell || selectedCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(selectedCell.id, true, 1)))
            break;
        case 'ArrowLeft':
            if (!selectedCell || selectedCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(selectedCell.id, false, -1)))
            break;
        case 'ArrowRight':
            if (!selectedCell || selectedCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            selectCell(document.getElementById(addId(selectedCell.id, false, 1)))
            break;
        case 'Tab':
            if (!selectedCell || selectedCell.hasAttribute("contenteditable")) {
                break;
            }
            event.preventDefault();
            let val = 1
            if (event.shiftKey) {
                val = -1
            }
            selectCell(document.getElementById(addId(selectedCell.id, false, val)))
            break;
    }
});

function saveCode(element) {
    localStorage.setItem(element.id, element.textContent)
}
