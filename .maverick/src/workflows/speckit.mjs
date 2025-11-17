// workflows/default.mjs
import path from 'path'
import { parsePhases, fileExists } from '../tasks/markdown.mjs'
import { executeStep, run } from '../steps/core.mjs'
import {
  opencodeImplementPhase,
  coderabbitReview,
  opencodeReview,
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
 * Build review steps (CodeRabbit and Opencode review).
 */
function buildReviewSteps({ projectRoot, outDir, reviewModel, coderabbitPath, reviewPath }) {
  return [
    coderabbitReview({
      cwd: projectRoot,
      captureTo: coderabbitPath,
    }),
    opencodeReview({
      cwd: projectRoot,
      captureTo: reviewPath,
      model: reviewModel,
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
  maxIterations = 3
}) {

  // Determine output directory and project root early (needed for cwd in phase commands)
  const outDir = tasksPath ? path.dirname(tasksPath) : process.cwd()
  const projectRoot = path.resolve(outDir, '..', '..')
  const coderabbitPath = path.join(outDir, 'coderabbit.md')
  const reviewPath = path.join(outDir, 'review.md')

  if (verbose) logger('Starting workflow with git checkout...')
  await run('git', ['checkout', '-B', branch])

  // Outer loop: iterate until all tasks complete or max iterations reached
  let iteration = 0
  let phases = initialPhases

  while (iteration < maxIterations) {
    iteration += 1
    
    // Re-parse tasks.md on subsequent iterations to check for updates
    if (iteration > 1) {
      if (verbose) logger(`Re-parsing tasks.md for iteration ${iteration}...`)
      phases = await parsePhases(tasksPath, process.cwd())
    }

    const phasesToRun = (phases || []).filter(p => (p.outstandingTasks ?? 0) > 0)
    const skippedCount = (phases?.length || 0) - phasesToRun.length
    
    if (phasesToRun.length === 0) {
      if (verbose) logger(`All tasks completed! Exiting after ${iteration} iteration(s).`)
      break
    }

    if (verbose) logger(`[Iteration ${iteration}/${maxIterations}] Running workflow on branch ${branch} for ${phasesToRun.length} phase(s) with outstanding tasks (of ${phases.length} total, ${skippedCount} skipped)`)

    // Build and execute phase steps
    const phaseSteps = buildPhaseSteps({ phasesToRun, projectRoot, buildModel, verbose, logger })
    
    for (const step of phaseSteps) {
      await executeStep(step, { logger, verbose })
      
      // Sleep after each phase
      if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before next step...`)
      await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))
    }

    // Check if we should continue to next iteration
    if (iteration < maxIterations) {
      const shouldContinue = (await parsePhases(tasksPath, process.cwd())).some(p => (p.outstandingTasks ?? 0) > 0)
      if (!shouldContinue) {
        if (verbose) logger(`All tasks completed! Exiting after ${iteration} iteration(s).`)
        break
      }
    }
  }

  // Build and execute review steps (skip if files already exist)
  const reviewSteps = buildReviewSteps({ projectRoot, outDir, reviewModel, coderabbitPath, reviewPath })
  
  for (const step of reviewSteps) {
    // Check if output file exists (skip if it does)
    if (step.captureTo && await fileExists(step.captureTo)) {
      if (verbose) logger(`${step.captureTo} exists; skipping ${step.id}`)
      continue
    }
    
    await executeStep(step, { logger, verbose })
    
    // Sleep after each review step
    if (verbose) logger(`Waiting ${PHASE_SLEEP_SECONDS}s before next step...`)
    await new Promise(resolve => setTimeout(resolve, PHASE_SLEEP_SECONDS * 1000))
  }

  // Build and execute fix step
  const fixStep = buildFixStep({ projectRoot, fixModel })
  await executeStep(fixStep, { logger, verbose })
}
