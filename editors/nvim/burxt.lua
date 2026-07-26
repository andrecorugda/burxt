-- Neovim: `require('burxt')` from your config, or paste this into init.lua.
--
-- Attaches `burxt lsp` to .bx buffers with no plugin manager and no
-- nvim-lspconfig dependency — vim.lsp.start is enough. Highlighting needs a
-- tree-sitter grammar, which does not exist yet, so this gives you diagnostics
-- as you type but not colour.

vim.filetype.add({ extension = { bx = "burxt" } })

vim.api.nvim_create_autocmd("FileType", {
  pattern = "burxt",
  callback = function(args)
    vim.lsp.start({
      name = "burxt-lsp",
      cmd = { "burxt", "lsp" },
      root_dir = vim.fs.dirname(args.file),
    }, { bufnr = args.buf })

    -- Match the language's own conventions: four spaces, `//` comments.
    vim.bo[args.buf].commentstring = "// %s"
    vim.bo[args.buf].expandtab = true
    vim.bo[args.buf].shiftwidth = 4
    vim.bo[args.buf].tabstop = 4
  end,
})
