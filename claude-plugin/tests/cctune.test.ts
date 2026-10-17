import { expect, test } from 'claude-code/testing'
import { answer, type Run, words } from '../hooks/register'

// A fake cctune answers: no program runs, the Music app is never touched.

function fake(exitCode: number, stdout: string, stderr = ''): Run & { calls: string[][] } {
  const calls: string[][] = []
  const run = async (argv: readonly string[]) => {
    calls.push([...argv])

    return { exitCode, stdout, stderr }
  }

  return Object.assign(run, { calls })
}

test('splits the words, a quoted part kept whole', () => {
  expect(words('playlist "Eras Tour"')).toEqual(['playlist', 'Eras Tour'])
  expect(words("play 'get lucky'")).toEqual(['play', 'get lucky'])
  expect(words('  vol   40 ')).toEqual(['vol', '40'])
  expect(words('')).toEqual([])
})

test('runs cctune with the words and shows its answer', async () => {
  const run = fake(0, 'playing the playlist Eras Tour\n')
  expect(await answer(run, 'cctune', 'playlist "Eras Tour"')).toBe('playing the playlist Eras Tour')
  expect(run.calls).toEqual([['cctune', 'playlist', 'Eras Tour']])
})

test('shows the error of a failed command', async () => {
  expect(await answer(fake(2, '', 'cctune: unknown command "nxt"\n'), 'cctune', 'nxt')).toBe('unknown command "nxt"')
})

test('does not repeat the name Claude Code already shows', async () => {
  expect(await answer(fake(0, 'cctune: control Apple Music\n\nUSAGE:\n'), 'cctune', 'help')).toBe('control Apple Music\n\nUSAGE:')
})

test('says how to install it when it cannot run', async () => {
  const missing: Run = async () => {
    throw new Error('not found')
  }
  expect(await answer(missing, 'cctune', 'next')).toContain('brew install pcangemi/tap/cctune')
})

test('/cctune is answered by the mod itself, with nothing for Claude', async $ => {
  const typed = { origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 120 } } as const
  const out = await $.command.run({ command: 'cctune', args: '--version', ...typed })
  expect(typeof out.text).toBe('string')
  expect(out.context).toBeUndefined()
})
