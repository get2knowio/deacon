// workflows/default.mjs
import path from 'path'
import { execa } from 'execa'
import { parsePhases, fileExists } from '../tasks/markdown.mjs'
import { executeStep, run } from '../steps/core.mjs'
import { makeOpencodeStep } from '../steps/opencode.mjs'
import {
  opencodeImplementPhase,
  codexReview,
  codexCoderabbitReview,
  codexOrganizeFixes,
  opencodeFix,
} from '../steps/speckit.mjs'

// Default sleep between phases in seconds
export const PHASE_SLEEP_SECONDS = 5

/**
 * Build phase execution steps from parsed phase objects.
 */
function buildPhaseSteps({ phasesToRun, projectRoot, buildModel, verbose, logger }) {
  return phasesToRun.map(phase => {
    if (verbose) {
      const counts = `(${phase.outstandingTasks}/${phase.totalTasks} outstanding)`
      logger(`Building step for phase ${phase.identifier} ${counts}: ${phase.title}`)
    }
    
    return opencodeImplementPhase(phase.identifier, {
      cwd: projectRoot,
      model: buildModel,
      outstandingTasks: phase.outstandingTasks,
      totalTasks: phase.totalTasks,
      phaseTitle: phase.title,
    })
  })
}

/**
 * Build review steps (4-step codex-based review process).
 */
function buildReviewSteps({ projectRoot }) {
  return [
    codexReview({
      cwd: projectRoot,
    }),
    codexCoderabbitReview({
      cwd: projectRoot,
    }),
    codexOrganizeFixes({
      cwd: projectRoot,
    }),
  ]
}

/**
 * Build fix step to address review findings.
 */
function buildFixStep({ projectRoot, fixModel }) {
  return opencodeFix({
    cwd: projectRoot,
    model: fixModel,
  })
}

/**
 * Run the default workflow: parse tasks, execute phases, review, and fix.
 * @param {object} config - Workflow configuration
 */
export async function runDefaultWorkflow({
  branch,
  phases: initialPhases,
  tasksPath,
  buildModel = 'github-copilot/claude-sonnet-4.5',
  reviewModel = 'github-copilot/claude-sonnet-4.5',
  fixModel = 'github-copilot/claude-sonnet-4.5',
  verbose = false,
  logger = m => console.log(m),
  maxRetriesPerPhase = 3
}) {

  // Determine output directory and project root early (needed for cwd in phase commands)
  const outDir = tasksPath ? path.dirname(tasksPath) : process.cwd()
  const projectRoot = path.resolve(outDir, '..', '..')

  if (verbose) logger('Starting workflow with git checkout...')
  await run('git', ['checkout', '-B', branch])

  // Parse initial phases
  let phases = initialPhases

  // Process each phase sequentially
  for (let phaseIndex = 0; phaseIndex < phases.length; phaseIndex++) {
    let retryCount = 0
    let phaseComplete = false

    while (!phaseComplete && retryCount < maxRetriesPerPhase) {
      retryCount += 1

      // Re-parse tasks.md to get current status
      const currentPhases = await parsePhases(tasksPath, process.cwd())
      const currentPhase = currentPhases[phaseIndex]

      if (!currentPhase || (currentPhase.outstandingTasks ?? 0) === 0) {
        if (verbose) logger(`Phase ${currentPhase?.identifier || phaseIndex + 1} completed!`)
        phaseComplete = true
        break
      }

      const attemptLabel = retryCount > 1 ? ` (attempt ${retryCount}/${maxRetriesPerPhase})` : ''
      const counts = `(${currentPhase.outstandingTasks}/${currentPhase.totalTasks} outstanding)`
      if (verbose) logger(`Running phase ${currentPhase.identifier}${attemptLabel} ${counts}: ${currentPhase.title}`)

      // Build and execute phase step
      const phaseStep = opencodeImplementPhase(currentPhase.identifier, {
        cwd: projectRoot,
        model: buildModel,
        outstandingTasks: currentPhase.outstandingTasks,
        totalTasks: currentPhase.totalTasks,
        phaseTitle: currentPhase.title,
      })

      await executeStep(phaseStep, { logger, verbose })

      // Commit the work for this phase attempt
      try {
        if (verbose) logger(`Committing work for phase ${currentPhase.identifier}...`)
        await run('git', ['add', '-A'])
        const commitMsg = `Phase ${currentPhase.identifier}: ${currentPhase.title}${attemptLabel}`
        await run('git', ['commit', '-m', commitMsg, '--allow-empty'])
        if (verbose) logger(`Committed: ${commitMsg}`)
      } catch (error) {
        if (verbose) logger(`Git commit failed (may be nothing to commit): ${error.message}`)
      }

      // Sleep before checking status or retrying
      if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before continuing...`)
      await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))

      // Re-check if phase is now complete
      const updatedPhases = await parsePhases(tasksPath, process.cwd())
      const updatedPhase = updatedPhases[phaseIndex]
      
      if (!updatedPhase || (updatedPhase.outstandingTasks ?? 0) === 0) {
        if (verbose) logger(`Phase ${currentPhase.identifier} completed after ${retryCount} attempt(s)!`)
        phaseComplete = true
      } else if (retryCount >= maxRetriesPerPhase) {
        if (verbose) logger(`Phase ${currentPhase.identifier} reached max retries (${maxRetriesPerPhase}). Moving to next phase.`)
        phaseComplete = true
      }
    }
  }

  // Build and execute review steps (3 codex steps)
  const reviewSteps = buildReviewSteps({ projectRoot })
  
  for (const step of reviewSteps) {
    await executeStep(step, { logger, verbose })
    
    // Sleep after each review step
    if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before next step...`)
    await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))
  }

  // Commit the organized fixes.md
  try {
    if (verbose) logger('Committing organized fixes.md...')
    await run('git', ['add', '-A'])
    await run('git', ['commit', '-m', 'Review: organized fixes.md with parallelization markers', '--allow-empty'])
    if (verbose) logger('Committed organized fixes.md')
  } catch (error) {
    if (verbose) logger(`Git commit failed (may be nothing to commit): ${error.message}`)
  }

  // Update constitution based on findings
  const constitutionStep = makeOpencodeStep(
    'opencode-update-constitution',
    'Evaluate the issues found in fixes.md and suggest updates to our constitution and AGENTS.md to prevent these from happening again.',
    {
      model: fixModel,
      command: 'speckit.constitution',
      cwd: projectRoot,
      label: 'opencode-update-constitution',
    }
  )
  await executeStep(constitutionStep, { logger, verbose })

  // Commit constitution updates
  try {
    if (verbose) logger('Committing constitution updates...')
    await run('git', ['add', '-A'])
    await run('git', ['commit', '-m', 'Review: updated constitution and AGENTS.md based on findings', '--allow-empty'])
    if (verbose) logger('Committed constitution updates')
  } catch (error) {
    if (verbose) logger(`Git commit failed (may be nothing to commit): ${error.message}`)
  }

  // Build and execute fix step
  const fixStep = buildFixStep({ projectRoot, fixModel })
  await executeStep(fixStep, { logger, verbose })

  // Commit the implemented fixes
  try {
    if (verbose) logger('Committing implemented fixes...')
    await run('git', ['add', '-A'])
    await run('git', ['commit', '-m', 'Review: implemented fixes from fixes.md', '--allow-empty'])
    if (verbose) logger('Committed implemented fixes')
  } catch (error) {
    if (verbose) logger(`Git commit failed (may be nothing to commit): ${error.message}`)
  }

  // Run test suite and fix any failures
  try {
    if (verbose) logger('Running test suite...')
    await run('make', ['test-nextest'], { cwd: projectRoot, timeout: 0 })
    if (verbose) logger('Tests passed!')
  } catch (error) {
    if (verbose) logger('Tests failed, capturing output and invoking opencode to fix...')
    
    // Capture the test output
    const testResult = await execa('make', ['test-nextest'], { 
      cwd: projectRoot, 
      reject: false,
      encoding: 'utf8',
      timeout: 0
    })
    const testOutput = `${testResult.stdout}\n${testResult.stderr}`.trim()
    
    // Create fix step with test output in prompt
    const testFixStep = makeOpencodeStep(
      'opencode-fix-tests',
      `The test suite failed with the following output:\n\n${testOutput}\n\nFix all failing tests and ensure "make test-nextest" passes.`,
      {
        model: fixModel,
        cwd: projectRoot,
        label: 'opencode-fix-tests',
      }
    )
    await executeStep(testFixStep, { logger, verbose })
    
    // Commit the test fixes
    try {
      if (verbose) logger('Committing test fixes...')
      await run('git', ['add', '-A'])
      await run('git', ['commit', '-m', 'Tests: fixed test failures', '--allow-empty'])
      if (verbose) logger('Committed test fixes')
    } catch (error) {
      if (verbose) logger(`Git commit failed (may be nothing to commit): ${error.message}`)
    }
    
    // Verify tests pass after fixes
    if (verbose) logger('Re-running tests to verify fixes...')
    await run('make', ['test-nextest'], { cwd: projectRoot, timeout: 0 })
    if (verbose) logger('Tests now pass!')
  }
}
