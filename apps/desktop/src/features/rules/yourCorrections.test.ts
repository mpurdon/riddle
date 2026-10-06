import { describe, expect, it } from 'vitest'
import { domainOf, guardLine, parseCorrections, withCorrection, withoutCorrection, YOUR_CORRECTIONS_MESH } from './yourCorrections'

describe('the Your corrections Mesh', () => {
  it('starts from nothing with a header and one guard line', () => {
    const text = withCorrection(null, guardLine('sender', 'Alerts@Uptime.example', 'spoil'))
    expect(text).toContain(`Mesh: ${YOUR_CORRECTIONS_MESH} v1`)
    expect(text).toContain('GUARDS\nNEVER Spoil IF sender = "alerts@uptime.example"')
    expect(parseCorrections(text)).toEqual([{ never: 'spoil', scope: 'sender', value: 'alerts@uptime.example', line: 'NEVER Spoil IF sender = "alerts@uptime.example"' }])
  })
  it('adds a second rule once and removes rules down to nothing', () => {
    const one = withCorrection(null, guardLine('sender', 'a@x.example', 'spoil'))
    const two = withCorrection(one, guardLine('domain', '@x.example', 'contamination'))
    expect(withCorrection(two, guardLine('domain', 'x.example', 'contamination'))).toBe(two)
    expect(parseCorrections(two).map((r) => `${r.never}:${r.scope}:${r.value}`)).toEqual(['spoil:sender:a@x.example', 'contamination:domain:x.example'])
    const back = withoutCorrection(two, guardLine('sender', 'a@x.example', 'spoil'))
    expect(parseCorrections(back!)).toHaveLength(1)
    expect(withoutCorrection(back!, guardLine('domain', 'x.example', 'contamination'))).toBeNull()
  })
  it('reads the domain of an address', () => {
    expect(domainOf('Jamie.O@Northwind.example')).toBe('northwind.example')
  })
})
