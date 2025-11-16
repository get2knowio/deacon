// workflow-core.mjs
import fs from 'fs/promises'
import path from 'path'
import { execa } from 'execa'

// Default sleep between phases in seconds
export const PHASE_SLEEP_SECONDS = 5

export async function parsePhases(tasksFile, baseDir) {
  const fullPath = path.isAbsolute(tasksFile)
    ? tasksFile
    : path.join(baseDir, tasksFile)

  const raw = await fs.readFile(fullPath, 'utf8')
  const lines = raw.split(/\r?\n/)

  const phases = []
  let current = null

  for (const line of lines) {
    const trimmed = line.trim()
    const m = /^##\s+Phase\s+([^:]+):(.*)$/.exec(trimmed)
    if (m) {
      if (current) {
        current.bodyText = current.body.join('\n').trim()
        current.tasks = extractTasks(current.body)
        current.totalTasks = current.tasks.length
        current.outstandingTasks = countOutstandingTasks(current.tasks)
        phases.push(current)
      }
      current = {
        identifier: m[1].trim(),
        title: m[2].trim(),
        body: []
      }
      continue
    }

    if (current) current.body.push(line)
  }

  if (current) {
    current.bodyText = current.body.join('\n').trim()
    current.tasks = extractTasks(current.body)
    current.totalTasks = current.tasks.length
    current.outstandingTasks = countOutstandingTasks(current.tasks)
    phases.push(current)
  }

  return phases
}

async function fileExists(filePath) {
  try {
    await fs.access(filePath)
    return true
  } catch {
    return false
  }
}

function extractTasks(lines) {
  return lines
    .map(l => l.trim())
    .filter(l => l.startsWith('- ['))
}

function countOutstandingTasks(tasks) {
  // A task is considered completed if it starts with "- [x]" or "- [X]"
  // Anything else (e.g., "- [ ]") is outstanding.
  let outstanding = 0
  for (const t of tasks) {
    const completed = /^- \[[xX]\]/.test(t)
    if (!completed) outstanding += 1
  }
  return outstanding
}

export function run(cmd, args = [], opts = {}) {
  return execa(cmd, args, {
    stdout: 'inherit',
    stderr: 'inherit',
    stdin: 'inherit',
    ...opts
  })
}

// Run a command inheriting stdin, stream stdout/stderr to parent, and capture stdout text
export async function runAndCapture(cmd, args = [], opts = {}) {
  const { teeToFile, ...spawnOpts } = opts
  const child = execa(cmd, args, {
    stdin: 'inherit',
    stdout: 'pipe',
    stderr: 'pipe',
    ...spawnOpts
  })
  let out = ''
  let err = ''
  let writeStream = null
  if (teeToFile) {
    writeStream = (await import('fs')).createWriteStream(teeToFile, { encoding: 'utf8' })
  }
  if (child.stdout) {
    child.stdout.on('data', chunk => {
      const s = chunk.toString()
      out += s
      process.stdout.write(s)
      if (writeStream) writeStream.write(s)
    })
  }
  if (child.stderr) {
    child.stderr.on('data', chunk => {
      const s = chunk.toString()
      err += s
      process.stderr.write(s)
    })
  }
  await child
  if (writeStream) writeStream.end()
  return { stdout: out, stderr: err }
}

export async function runWithProgress(cmd, args = [], { logger = m => console.log(m), label, intervalMs = 5000, opts = {}, showExec = true } = {}) {
  const started = Date.now()
  if (showExec) {
    logger(`→ exec: ${cmd} ${args.join(' ')}`)
  }
  const child = execa(cmd, args, {
    stdout: 'inherit',
    stderr: 'inherit',
    stdin: 'inherit',
    ...opts
  })
  const tag = label || cmd
  const timer = setInterval(() => {
    logger(`${tag} running ${(Date.now() - started) / 1000}s`)
  }, intervalMs)
  try {
    await child
    logger(`${tag} finished in ${(Date.now() - started) / 1000}s`)
  } finally {
    clearInterval(timer)
  }
}

export async function runWorkflow({
  branch,
  phases,
  tasksPath,
  buildModel = 'github-copilot/claude-sonnet-4.5',
  reviewModel = 'github-copilot/claude-sonnet-4.5',
  fixModel = 'github-copilot/claude-sonnet-4.5',
  verbose = false,
  logger = m => console.log(m)
}) {

  if (verbose) logger('Starting workflow with git checkout...')
  // this is pure “business logic”
  await run('git', ['checkout', '-B', branch])

  const phasesToRun = (phases || []).filter(p => (p.outstandingTasks ?? 0) > 0)
  const skippedCount = (phases?.length || 0) - phasesToRun.length
  if (verbose) logger(`Running workflow on branch ${branch} for ${phasesToRun.length} phase(s) with outstanding tasks (of ${phases.length} total, ${skippedCount} skipped)`) 

  for (let i = 0; i < phasesToRun.length; i += 1) {
    const phase = phasesToRun[i]
    const counts = `(${phase.outstandingTasks}/${phase.totalTasks} outstanding)`
    if (verbose) logger(`Starting phase ${phase.identifier} ${counts}: ${phase.title}`)
    const prompt =
      `implement phase ${phase.identifier} tasks, ` +
      'updating tasks.md as you complete each task. Do not stop until all the tasks for this phase have been completed.'
    const phaseArgs = [
      'run',
      '--model',
      buildModel,
      '--command',
      'speckit.implement',
      prompt
    ]
    if (verbose) {
      const quoted = phaseArgs.map(a => (a.includes(' ') ? `"${a}"` : a)).join(' ')
      logger(`CMD: opencode ${quoted} (cwd=${process.cwd()})`)
    }
    await runWithProgress(
      'opencode',
      phaseArgs,
      { logger: verbose ? logger : () => {}, label: `phase-${phase.identifier}`, showExec: false, opts: { cwd: projectRoot } }
    )

    // Sleep between phases except after the last one
    if (i < phasesToRun.length - 1) {
      if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before next phase...`)
      await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))
    }
  }

  // Determine output directory (same directory as tasks.md)
  const outDir = tasksPath ? path.dirname(tasksPath) : process.cwd()
  const projectRoot = path.resolve(outDir, '..', '..')
  const coderabbitPath = path.join(outDir, 'coderabbit.md')
  const reviewPath = path.join(outDir, 'review.md')

  // Sleep before review phase
  if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before review phase...`)
  await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))

  // Step 1: Run CodeRabbit review and write to coderabbit.md
  if (!(await fileExists(coderabbitPath))) {
    if (verbose) logger('Running CodeRabbit review (prompt-only) → coderabbit.md')
    if (verbose) logger(`CMD: coderabbit review --prompt-only (cwd=${projectRoot})`)
    await runAndCapture('coderabbit', ['review', '--prompt-only'], { cwd: projectRoot, teeToFile: coderabbitPath })
    if (verbose) logger('CodeRabbit review saved to coderabbit.md')
  } else {
    if (verbose) logger('coderabbit.md exists; skipping CodeRabbit review')
  }

  // Sleep before standalone review
  if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before standalone review...`)
  await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))

  // Step 2: Run an Opencode prompt to perform an independent code review → review.md
  if (!(await fileExists(reviewPath))) {
    if (verbose) logger('Running Opencode standalone code review → review.md')
    const reviewPrompt = [
      'Perform a standalone, senior-level code review of this repository, measuring both quality and completeness.',
      'Use spec.md, plan.md, and tasks.md in the current speckit spec directory as guidance.',
      'Provide actionable findings with severity, rationale, and concrete fixes.',
      'Include file paths and line ranges when possible. Do not make changes.',
      'Output only the review content in the format of prompts that an AI agent can act on (no extra chatter).'
    ].join(' ')
    const opencodeArgs = [
      'run',
      '--model',
      reviewModel,
      reviewPrompt
    ]
    if (verbose) logger(`CMD: opencode ${opencodeArgs.map(a => a.includes(' ') ? '"'+a+'"' : a).join(' ')} (cwd=${projectRoot})`)
    await runAndCapture('opencode', opencodeArgs, { cwd: projectRoot, teeToFile: reviewPath })
    if (verbose) logger('Opencode review saved to review.md')
  } else {
    if (verbose) logger('review.md exists; skipping Opencode review')
  }

  // Sleep before fix phase
  if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before fix phase...`)
  await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))

  // Step 3: Run an Opencode prompt to address issues from coderabbit.md and review.md
  if (verbose) logger('Running Opencode to address issues from coderabbit.md and review.md')
  const fixPrompt = [
    'Read coderabbit.md and review.md (in the current speckit spec directory).',
    'Address all issues raised by implementing code changes where appropriate.',
    'Do not remove important validations, tests, or safety checks.',
    'Prefer small, incremental commits; keep style consistent; no unrelated refactors.'
  ].join(' ')
  const fixArgs = [
    'run',
    '--model',
    fixModel,
    fixPrompt
  ]
  if (verbose) logger(`CMD: opencode ${fixArgs.map(a => a.includes(' ') ? '"'+a+'"' : a).join(' ')} (cwd=${projectRoot})`)
  await runWithProgress(
    'opencode',
    fixArgs,
    { logger: verbose ? logger : () => {}, label: 'opencode-fix-issues', opts: { cwd: projectRoot } }
  )

  // await run('git', ['status'])
  // await run('git', ['add', '.'])
  // try {
  //   await run('git', ['commit', '-m', 'Apply build subcommand parity tasks'])
  // } catch {
  //   // no-op if nothing to commit
  // }
  // await run('git', ['checkout', 'main'])
  // await run('git', ['merge', branch])
}