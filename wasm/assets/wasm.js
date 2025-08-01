function getApiBaseUrl() {
    const hostname = window.location.hostname;

    if (hostname === 'localhost' || hostname === '127.0.0.1') {
        return 'http://localhost:8080/api';
    }

    return '$$API_URL$$';
}

window.apiBaseUrl = getApiBaseUrl();

// This function is called from Rust to evaluate the user functions.
// TODO: I am really not sure about performance of this, it is a working prototype.
window.js_evaluate = function (fnName, vars) {
    return window.userWasmModule[fnName](...vars)
}

async function loadWasmBindgenModule(jsUrl, wasmUrl) {
    const module = await import(jsUrl);
    await module.default({module_or_path: wasmUrl});
    window.userWasmModule = module;
    return module;
}

function appendLog(logsContainer, message) {
    const logEntry = document.createElement('p');
    logEntry.textContent = message;
    logEntry.className = 'log-entry';
    logsContainer.prepend(logEntry);
    logsContainer.scrollTop = logsContainer.scrollHeight;
}

function setResult(message, loading = false) {
    const el = document.getElementById('my-result');
    el.textContent = message
    el.className = loading ? "loading" : "";
}

async function compile() {
    setResult("Compiling", true);

    let url = `${window.apiBaseUrl}/compile`
    let workspaceId = localStorage.getItem("workspace-id");
    if (workspaceId !== null) {
        url = url + `?workspace_id=${workspaceId}`
    }

    const response = await fetch(url, {
        method: 'PUT',
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify({
            lib_rs: document.getElementById("lib-rs-content").textContent,
            cargo_toml: document.getElementById("cargo-toml-content").textContent,
        })
    })

    if (!response.ok) {
        if (response.status === 404) {
            localStorage.removeItem("workspace-id");
        }
        setResult(`HTTP error: ${response.status} (try pressing F5)`);
        return;
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    const logsContainer = document.getElementById('logs');
    while (true) {
        const {done, value} = await reader.read();

        if (done) {
            break;
        }

        // Decode the chunk and add to buffer
        buffer += decoder.decode(value, {stream: true});

        // Process complete lines
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        function stripDataPrefix(line) {
            const PREFIX = "data: ";
            if (line.startsWith(PREFIX)) {
                line = line.slice(PREFIX.length);
            }
            return line
        }

        for (const line of lines) {
            const parsed = JSON.parse(stripDataPrefix(line));
            if (parsed.stdout_line !== undefined) {
                appendLog(logsContainer, parsed.stdout_line);
            } else if (parsed.stderr_line !== undefined) {
                appendLog(logsContainer, parsed.stderr_line);
            } else if (parsed.log !== undefined) {
                appendLog(logsContainer, parsed.log);
            } else if (parsed.error !== undefined) {
                setResult(`Compile error: ${parsed.error}`);
                return;
            } else if (parsed.download_info !== undefined) {
                await loadWasmBindgenModule(
                    `${window.apiBaseUrl}${parsed.download_info.js_download_url}`,
                    `${window.apiBaseUrl}${parsed.download_info.wasm_download_url}`,
                );
                localStorage.setItem("workspace-id", parsed.download_info.workspace_id);
                setResult("Successfully compiled, enter your expression above.");
                window.wasmBindings.init_app();
            } else {
                console.error("unknown SSE message:", parsed)
            }
        }
    }
}

function loadCodeOrSetDefault(key, defaultContent) {
    let content = localStorage.getItem(key)
    if (!content) {
        content = defaultContent
    }
    const element = document.getElementById(key)
    element.textContent = content
    localStorage.setItem(key, content)
}

loadCodeOrSetDefault(
    "cargo-toml-content",
    `[package]
name = "sheeet-lib"
edition = "2024"

[dependencies]
wasm-bindgen = "0.2.100"

[lib]
crate-type = ["cdylib", "rlib"]
`
)
loadCodeOrSetDefault(
    "lib-rs-content",
    `use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(a: f64, b: f64) -> f64 {
    a + b
}

#[wasm_bindgen]
pub fn sub(a: f64, b: f64) -> f64 {
    a - b
}
`
)

await compile();