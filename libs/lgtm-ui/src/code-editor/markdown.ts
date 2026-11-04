import { codeEditorMarkdownStyles } from './styles'

const INLINE_TOKEN_RE =
  /(`[^`\n]+`|\*\*\*[^*\n]+\*\*\*|\*\*[^*\n]+\*\*|\*[^*\n]+\*|\[[^\]\n]+\]\(https?:\/\/[^\s)]+\))/g

function appendInlineMarkdown(parent: Node, text: string) {
  const lines = text.split('\n')
  lines.forEach((line, lineIdx) => {
    if (lineIdx > 0) {
      parent.appendChild(document.createElement('br'))
    }
    let lastIndex = 0
    for (const match of line.matchAll(INLINE_TOKEN_RE)) {
      const idx = match.index ?? 0
      if (idx > lastIndex) {
        parent.appendChild(document.createTextNode(line.slice(lastIndex, idx)))
      }
      const token = match[0]
      if (token.startsWith('`') && token.endsWith('`')) {
        const code = document.createElement('code')
        code.className = codeEditorMarkdownStyles.inlineCode
        code.textContent = token.slice(1, -1)
        parent.appendChild(code)
      } else if (token.startsWith('***') && token.endsWith('***')) {
        const strong = document.createElement('strong')
        strong.className = codeEditorMarkdownStyles.strong
        const em = document.createElement('em')
        em.textContent = token.slice(3, -3)
        strong.appendChild(em)
        parent.appendChild(strong)
      } else if (token.startsWith('**') && token.endsWith('**')) {
        const strong = document.createElement('strong')
        strong.className = codeEditorMarkdownStyles.strong
        strong.textContent = token.slice(2, -2)
        parent.appendChild(strong)
      } else if (token.startsWith('*') && token.endsWith('*')) {
        const em = document.createElement('em')
        em.textContent = token.slice(1, -1)
        parent.appendChild(em)
      } else if (token.startsWith('[')) {
        const closeBracket = token.indexOf('](')
        if (closeBracket !== -1) {
          const label = token.slice(1, closeBracket)
          const href = token.slice(closeBracket + 2, -1)
          const anchor = document.createElement('a')
          anchor.className = codeEditorMarkdownStyles.link
          anchor.href = href
          anchor.target = '_blank'
          anchor.rel = 'noreferrer'
          anchor.textContent = label
          parent.appendChild(anchor)
        } else {
          parent.appendChild(document.createTextNode(token))
        }
      } else {
        parent.appendChild(document.createTextNode(token))
      }
      lastIndex = idx + token.length
    }
    if (lastIndex < line.length) {
      parent.appendChild(document.createTextNode(line.slice(lastIndex)))
    }
  })
}

export function renderMarkdownToDom(markdown: string): DocumentFragment {
  const fragment = document.createDocumentFragment()
  const lines = markdown.replace(/\r\n/g, '\n').split('\n')

  let i = 0
  while (i < lines.length) {
    const line = lines[i]!
    const trimmed = line.trim()

    if (!trimmed) {
      i++
      continue
    }

    if (trimmed.startsWith('```')) {
      const lang = trimmed.slice(3).trim()
      const codeLines: string[] = []
      i++
      while (i < lines.length && !lines[i]!.trim().startsWith('```')) {
        codeLines.push(lines[i]!)
        i++
      }
      if (i < lines.length) {
        i++
      }
      const pre = document.createElement('pre')
      pre.className = codeEditorMarkdownStyles.codeBlock
      const code = document.createElement('code')
      if (lang) {
        code.setAttribute('data-language', lang)
      }
      code.textContent = codeLines.join('\n')
      pre.appendChild(code)
      fragment.appendChild(pre)
      continue
    }

    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      const hr = document.createElement('hr')
      hr.className = codeEditorMarkdownStyles.hr
      fragment.appendChild(hr)
      i++
      continue
    }

    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/)
    if (headingMatch) {
      const heading = document.createElement('div')
      heading.className = codeEditorMarkdownStyles.heading
      appendInlineMarkdown(heading, headingMatch[2]!)
      fragment.appendChild(heading)
      i++
      continue
    }

    if (trimmed.startsWith('> ')) {
      const quoteLines: string[] = []
      while (i < lines.length && lines[i]!.trim().startsWith('>')) {
        quoteLines.push(lines[i]!.trim().replace(/^>\s?/, ''))
        i++
      }
      const blockquote = document.createElement('blockquote')
      blockquote.className = codeEditorMarkdownStyles.blockquote
      appendInlineMarkdown(blockquote, quoteLines.join('\n'))
      fragment.appendChild(blockquote)
      continue
    }

    if (/^[-*+]\s+/.test(trimmed)) {
      const ul = document.createElement('ul')
      ul.className = codeEditorMarkdownStyles.list
      while (i < lines.length && /^[-*+]\s+/.test(lines[i]!.trim())) {
        const li = document.createElement('li')
        appendInlineMarkdown(li, lines[i]!.trim().replace(/^[-*+]\s+/, ''))
        ul.appendChild(li)
        i++
      }
      fragment.appendChild(ul)
      continue
    }

    if (/^\d+\.\s+/.test(trimmed)) {
      const ol = document.createElement('ol')
      ol.className = codeEditorMarkdownStyles.orderedList
      while (i < lines.length && /^\d+\.\s+/.test(lines[i]!.trim())) {
        const li = document.createElement('li')
        appendInlineMarkdown(li, lines[i]!.trim().replace(/^\d+\.\s+/, ''))
        ol.appendChild(li)
        i++
      }
      fragment.appendChild(ol)
      continue
    }

    const paragraphLines: string[] = []
    while (
      i < lines.length &&
      lines[i]!.trim() !== '' &&
      !lines[i]!.trim().startsWith('```') &&
      !/^(-{3,}|\*{3,}|_{3,})$/.test(lines[i]!.trim()) &&
      !/^#{1,6}\s+/.test(lines[i]!.trim()) &&
      !lines[i]!.trim().startsWith('> ') &&
      !/^[-*+]\s+/.test(lines[i]!.trim()) &&
      !/^\d+\.\s+/.test(lines[i]!.trim())
    ) {
      paragraphLines.push(lines[i]!)
      i++
    }
    const p = document.createElement('p')
    p.className = codeEditorMarkdownStyles.paragraph
    appendInlineMarkdown(p, paragraphLines.join('\n'))
    fragment.appendChild(p)
  }

  return fragment
}
