// Vanilla ES-module client that mirrors
// apps/lgtm-web-tanstack-spa/src/lib/lgtm-client.ts and ReviewLayout.tsx.
// Keep behavior parity when either side changes.

const config = {
  baseUrl: 'http://127.0.0.1:50051',
  // Hardcoded default to the PoC fixture repo. Will be replaced by a Tauri
  // command / env read when the shell learns to pick a repo.
  repoPath: '/Users/siddharthkapoor/.herdr/worktrees/lgtm/poc-gpui-rpc-sqlite/.tmp/repos/ripgrep',
  baseRef: 'HEAD~5',
}

const els = {
  list: document.getElementById('filelist'),
  selected: document.getElementById('selected'),
  loading: document.getElementById('loading'),
  error: document.getElementById('error'),
  diff: document.getElementById('diff'),
  reload: document.getElementById('reload'),
}

let state = {
  files: [],
  selected: null,
}

async function unary(method, body) {
  const res = await fetch(`${config.baseUrl}/lgtm.v1.LgtmService/${method}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({}))
    throw new Error(err.message ?? `lgtm ${method} failed: ${res.status}`)
  }
  return res.json()
}

function setError(msg) {
  if (!msg) {
    els.error.hidden = true
    els.error.textContent = ''
    return
  }
  els.error.hidden = false
  els.error.textContent = msg
}

function setLoading(on) {
  els.loading.hidden = !on
}

function renderList() {
  els.list.innerHTML = ''
  if (!state.files.length) {
    const li = document.createElement('li')
    li.className = 'lgtm-empty'
    li.textContent = 'No changed files'
    els.list.appendChild(li)
    return
  }
  for (const file of state.files) {
    const li = document.createElement('li')
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.className = 'lgtm-fileitem'
    if (state.selected?.path === file.path) btn.classList.add('is-active')
    btn.onclick = () => selectFile(file)
    const status = document.createElement('span')
    status.className = `lgtm-status lgtm-status-${file.status}`
    status.textContent = file.status
    const path = document.createElement('span')
    path.className = 'lgtm-path'
    path.textContent = file.path
    btn.append(status, path)
    li.appendChild(btn)
    els.list.appendChild(li)
  }
}

async function loadFiles() {
  setLoading(true)
  setError(null)
  try {
    const { files } = await unary('GetChangedFiles', {
      repo_path: config.repoPath,
      base_ref: config.baseRef,
    })
    state.files = files
    if (!state.selected && files.length) selectFile(files[0])
    renderList()
  } catch (e) {
    setError(e.message)
  } finally {
    setLoading(false)
  }
}

async function selectFile(file) {
  state.selected = file
  els.selected.textContent = file.path
  renderList()
  setLoading(true)
  setError(null)
  try {
    const { diff_text } = await unary('GetFileDiff', {
      repo_path: config.repoPath,
      base_ref: config.baseRef,
      path: file.path,
    })
    els.diff.textContent = diff_text
  } catch (e) {
    setError(e.message)
  } finally {
    setLoading(false)
  }
}

els.reload.addEventListener('click', loadFiles)
loadFiles()
