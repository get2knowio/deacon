// steps/speckit.mjs
// Domain-specific step types for speckit workflow automation
// These build on generic step types to encapsulate complete workflow patterns
import { makeOpencodeStep } from './opencode.mjs'
import { makeCoderabbitStep } from './coderabbit.mjs'

/**
 * Create an opencode step that implements tasks for a specific phase.
 * Automatically constructs the appropriate prompt and command options.
 * 
 * @param {string} phaseIdentifier - Phase identifier from tasks.md (e.g., "1", "2A")
 * @param {object} opts - Options
 * @param {string} opts.cwd - Working directory
 * @param {string} opts.model - Model to use (default: github-copilot/claude-sonnet-4.5)
 * @param {number} opts.outstandingTasks - Number of outstanding tasks (for logging)
 * @param {number} opts.totalTasks - Total tasks in phase (for logging)
 * @param {string} opts.phaseTitle - Human-readable phase title (for logging)
 * @returns {object} Step descriptor
 */
export function opencodeImplementPhase(phaseIdentifier, opts = {}) {
  const {
    cwd,
    model = 'github-copilot/claude-sonnet-4.5',
    outstandingTasks,
    totalTasks,
    phaseTitle,
  } = opts

  const prompt =
    `implement phase ${phaseIdentifier} tasks, ` +
    'updating tasks.md as you complete each task. Do not stop until all the tasks for this phase have been completed.'

  const labelSuffix = outstandingTasks && totalTasks 
    ? ` (${outstandingTasks}/${totalTasks} outstanding)`
    : ''

  return makeOpencodeStep(
    `phase-${phaseIdentifier}`,
    prompt,
    {
      model,
      command: 'speckit.implement',
      cwd,
      label: `phase-${phaseIdentifier}${labelSuffix}`,
    }
  )
}

/**
 * Create a coderabbit review step that generates a prompt-only review.
 * Captures output to the specified file.
 * 
 * @param {object} opts - Options
 * @param {string} opts.cwd - Working directory
 * @param {string} opts.captureTo - File path to capture review output
 * @returns {object} Step descriptor
 */
export function coderabbitReview(opts = {}) {
  const { cwd, captureTo } = opts

  return makeCoderabbitStep(
    'coderabbit-review',
    ['review', '--prompt-only'],
    {
      cwd,
      captureTo,
      label: 'coderabbit-review',
    }
  )
}

/**
 * Create an opencode step that performs an independent senior-level code review.
 * Captures output to the specified file.
 * 
 * @param {object} opts - Options
 * @param {string} opts.cwd - Working directory
 * @param {string} opts.captureTo - File path to capture review output
 * @param {string} opts.model - Model to use (default: github-copilot/claude-sonnet-4.5)
 * @returns {object} Step descriptor
 */
export function opencodeReview(opts = {}) {
  const {
    cwd,
    captureTo,
    model = 'github-copilot/claude-sonnet-4.5',
  } = opts

  const prompt = [
    'Perform a standalone, senior-level code review of this repository, measuring both quality and completeness.',
    'Use spec.md, plan.md, and tasks.md in the current speckit spec directory as guidance.',
    'Provide actionable findings with severity, rationale, and concrete fixes.',
    'Include file paths and line ranges when possible. Do not make changes.',
    'Output only the review content in the format of prompts that an AI agent can act on (no extra chatter).'
  ].join(' ')

  return makeOpencodeStep(
    'opencode-review',
    prompt,
    {
      model,
      cwd,
      captureTo,
      label: 'opencode-review',
    }
  )
}

/**
 * Create an opencode step that addresses issues from review feedback.
 * Reads coderabbit.md and review.md to find issues, then implements fixes.
 * 
 * @param {object} opts - Options
 * @param {string} opts.cwd - Working directory
 * @param {string} opts.model - Model to use (default: github-copilot/claude-sonnet-4.5)
 * @returns {object} Step descriptor
 */
export function opencodeFix(opts = {}) {
  const {
    cwd,
    model = 'github-copilot/claude-sonnet-4.5',
  } = opts

  const prompt = [
    'Read coderabbit.md and review.md (in the current speckit spec directory).',
    'Address all issues raised by implementing code changes where appropriate.',
    'Do not remove important validations, tests, or safety checks.',
    'Prefer small, incremental commits; keep style consistent; no unrelated refactors.'
  ].join(' ')

  return makeOpencodeStep(
    'opencode-fix-issues',
    prompt,
    {
      model,
      cwd,
      label: 'opencode-fix-issues',
    }
  )
}
