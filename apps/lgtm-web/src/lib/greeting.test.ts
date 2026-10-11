import { describe, expect, test } from 'bun:test'
import { greeting } from './greeting'

describe('greeting', () => {
  test('greets the framework', () => {
    expect(greeting('SvelteKit')).toBe('Hello, SvelteKit!')
  })
})
