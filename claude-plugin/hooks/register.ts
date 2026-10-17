import type { ProcessRunResult, Register } from 'claude-code'

// `/cctune next`, `/cctune playlist "Eras Tour"`: runs the cctune program and shows its answer
// in the transcript. Nothing goes to Claude, and it runs at once, even while a turn streams.

/** The words after `/cctune`, a quoted part kept as one word (`playlist "Eras Tour"`). */
export function words(args: string): string[] {
  return [...args.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map(m => m[1] ?? m[2] ?? m[3] ?? '')
}

export type Run = (argv: readonly string[]) => Promise<Pick<ProcessRunResult, 'exitCode' | 'stdout' | 'stderr'>>

/** The text `/cctune <args>` shows: cctune's answer, its error, or how to install it. */
export async function answer(run: Run, binary: string, args: string): Promise<string> {
  try {
    const { exitCode, stdout, stderr } = await run([binary, ...words(args)])
    // Claude Code shows the answer under the plugin's name already: `cctune: cctune: …` would repeat it.
    const text = (exitCode === 0 ? stdout : stderr || stdout).trim().replace(/^cctune: /, '')

    return text || (exitCode === 0 ? 'done' : `cctune failed (exit ${exitCode})`)
  } catch (err) {
    const why = err instanceof Error ? err.message : String(err)

    return `cctune could not run (${binary}: ${why}). To install it: brew install pcangemi/tap/cctune`
  }
}

export const register: Register = (on, options) => {
  const binary = typeof options.binary === 'string' && options.binary !== '' ? options.binary : 'cctune'

  on('session.start', async ($, e, next) => {
    await $.command.register({
      name: 'cctune',
      description: 'Apple Music: what plays, next, pause, play SONG, playlist NAME, vol N, search…',
      argumentHint: '[next | pause | play SONG | playlist NAME | vol N | help]',
      immediate: true,
    })

    return next(e)
  })

  on('command.run', { command: 'cctune' }, async ($, e) => ({
    text: await answer(argv => $.process.run(argv, { timeoutMs: 15000 }), binary, e.args),
  }))
}
