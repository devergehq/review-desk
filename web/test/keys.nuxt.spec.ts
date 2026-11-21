import { afterEach, describe, expect, it } from 'vitest'

import { keyIsForThePage } from '~/utils/keys'

/*
 * **When a key is not a key** — `web/app/utils/keys.ts`, the one rule every page with keys
 * shares. The fourth way, added with the phone's conversation sheet (DEV-1336): nothing
 * open over the page, or the keys go to the page behind it.
 */

function press(key: string, target: EventTarget = document.body, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, ...init })
  Object.defineProperty(event, 'target', { value: target })
  return event
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('keyIsForThePage', () => {
  it('is the page’s when nothing is over it and nothing is being typed', () => {
    document.body.innerHTML = '<main id="main"><button>Post</button></main>'
    expect(keyIsForThePage(press('s', document.querySelector('button')!))).toBe(true)
  })

  it('is not the page’s in a field, with a modifier, or while composing', () => {
    document.body.innerHTML = '<textarea></textarea>'
    expect(keyIsForThePage(press('s', document.querySelector('textarea')!))).toBe(false)
    expect(keyIsForThePage(press('s', document.body, { metaKey: true }))).toBe(false)
    expect(keyIsForThePage(press('s', document.body, { isComposing: true }))).toBe(false)
  })

  it('is not the page’s while a modal dialog is open over it', () => {
    document.body.innerHTML = `
      <main id="main"></main>
      <aside id="conversation" role="dialog" aria-modal="true"><button>Close</button></aside>`
    expect(keyIsForThePage(press('s', document.querySelector('aside button')!))).toBe(false)
  })

  it('is not the page’s while the page is inert under the menu', () => {
    document.body.innerHTML = '<aside id="sidebar"><a href="/">Pull requests</a></aside><main id="main" inert></main>'
    expect(keyIsForThePage(press('s', document.querySelector('a')!))).toBe(false)
  })

  it('is the page’s again once the dialog has closed', () => {
    document.body.innerHTML = '<main id="main"></main><aside id="conversation"></aside>'
    expect(keyIsForThePage(press('s'))).toBe(true)
  })
})
