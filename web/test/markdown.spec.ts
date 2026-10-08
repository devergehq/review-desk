// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import MarkdownIt from 'markdown-it'
import DOMPurify from 'dompurify'

/*
 * How a draft is rendered. GitHub allows raw HTML in a review body, which is what keeps a
 * folded proof folded, so the renderer allows it too and then sanitises — a draft is
 * written by a session, and the one screen that exists so nothing is posted unread should
 * not be the one place a session can run script.
 *
 * The environment matters here and is pinned to jsdom in `vitest.config.ts`: happy-dom
 * drops `<details>` silently, which made a working sanitiser look broken.
 */

const md = new MarkdownIt({ html: true, linkify: true, typographer: false, breaks: false })

function render(markdown: string): string {
  return DOMPurify.sanitize(md.render(markdown), { ADD_ATTR: ['open'] })
}

describe('a folded section survives rendering', () => {
  const source = `<details>\n<summary>Proof</summary>\n\n\`\`\`php\n$x = 1;\n\`\`\`\n\n</details>\n`

  it('keeps the fold and its summary', () => {
    const html = render(source)
    expect(html).toContain('<details>')
    expect(html).toContain('<summary>Proof</summary>')
    expect(html).toContain('</details>')
  })

  it('leaves it closed, so the proof costs nothing to skip', () => {
    expect(render(source)).not.toContain('<details open')
  })

  it('lets a session ship a fold already open', () => {
    expect(render('<details open>\n<summary>s</summary>\n</details>')).toContain('open')
  })

  it('renders the code inside it as code', () => {
    expect(render(source)).toContain('<pre><code class="language-php">')
  })
})

describe('what the renderer refuses', () => {
  it('drops a script tag', () => {
    expect(render('<script>alert(1)</script>\n\nAfter.')).not.toContain('<script')
  })

  it('drops an inline event handler', () => {
    expect(render('<div onclick="steal()">x</div>')).not.toContain('onclick')
  })

  it('never emits a javascript: target', () => {
    /* markdown-it refuses the target itself and leaves the literal text, which is safe. */
    const html = render('[click](javascript:alert(1))')
    expect(html).not.toContain('href="javascript:')
    expect(html).toContain('click')
  })

  it('drops a javascript: target written as raw HTML', () => {
    const html = render('<a href="javascript:alert(1)">click</a>')
    expect(html).not.toContain('javascript:')
  })

  it('keeps an ordinary link', () => {
    expect(render('[the review](https://github.com/acme/portal/pull/412)')).toContain(
      'href="https://github.com/acme/portal/pull/412"',
    )
  })
})

describe('GitHub-flavoured prose', () => {
  it('renders headings, inline code, emphasis and tables', () => {
    expect(render('### A heading')).toContain('<h3>A heading</h3>')
    expect(render('`code`')).toContain('<code>code</code>')
    expect(render('**bold**')).toContain('<strong>bold</strong>')
    expect(render('| a | b |\n| -- | -- |\n| 1 | 2 |')).toContain('<table>')
  })
})
