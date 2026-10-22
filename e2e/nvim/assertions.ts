import { expect } from 'vitest'
import type { Editor } from './editor.js'
import { waitFor } from '../common/runtime.js'

export async function expectFileSeen(e: Editor, path: string, expected: boolean) {
  // Assert
  await e.wait(
    'not require("lgtm.context").current.reviewed_loading',
    'reviewed-file progress loaded',
  )
  expect(Boolean((await e.state())!.reviewed?.[path])).toBe(expected)
  await e.wait(
    '(function() local s=require("lgtm.context").current; local p=s and require("codediff.ui.lifecycle").get_panel_view(s.tab); return p and p.tree and vim.api.nvim_buf_is_valid(p.bufnr) end)()',
    'native CodeDiff file tree',
  )
  const visible = await waitFor(
    `visible checkmark for ${path}`,
    () =>
      e.lua<boolean>(
        `local path=...; local s=require("lgtm.context").current
    local p=require("codediff.ui.lifecycle").get_panel_view(s.tab)
    for _,mark in ipairs(vim.api.nvim_buf_get_extmarks(p.bufnr,vim.api.nvim_create_namespace("lgtm.reviewed"),0,-1,{details=true})) do
      local node=p.tree:get_node(mark[2]+1)
      if node and node.data.path==path then return mark[4].virt_text[1][1]:find("✓",1,true) ~= nil end
    end; return false`,
        path,
      ),
    (value) => value === expected,
    5000,
  )
  expect(visible, `visible checkmark for ${path}`).toBe(expected)
}
