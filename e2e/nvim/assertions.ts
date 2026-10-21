import { expect } from 'vitest'
import type { Editor } from './editor.js'

export async function expectFileSeen(e: Editor, path: string, expected: boolean) {
  // Assert
  expect(Boolean((await e.state())!.reviewed?.[path])).toBe(expected)
  const visible = await e.lua(
    `local path=...; local s=require("lgtm.context").current
    local p=require("codediff.ui.lifecycle").get_panel_view(s.tab)
    for _,mark in ipairs(vim.api.nvim_buf_get_extmarks(p.bufnr,vim.api.nvim_create_namespace("lgtm.reviewed"),0,-1,{details=true})) do
      local node=p.tree:get_node(mark[2]+1)
      if node and node.data.path==path then return mark[4].virt_text[1][1]:find("✓",1,true) ~= nil end
    end; return false`,
    path,
  )
  expect(visible, `visible checkmark for ${path}`).toBe(expected)
}
