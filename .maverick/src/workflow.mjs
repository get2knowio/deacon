#!/usr/bin/env node
import { Listr } from 'listr2'
import meow from 'meow'
import { parsePhases, runWorkflow } from './workflow-core.mjs'
import { fileURLToPath } from 'url'
import path from 'path'
import { existsSync } from 'fs'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

async function main() {
  const cli = meow(
    `\n  Usage\n    $ maverick [branch] [options]\n\n  Positional Args\n    branch     Branch name to work on (default: build-parity-opencode)\n\n  Default Tasks Path\n    specs/<branch>/tasks.md (override with --tasks)\n\n  Options\n    --branch, -b    Override branch name\n    --tasks, -t     Override tasks file path\n    --verbose, -v   Enable verbose internal logging (phase summaries, workflow steps)\n    --help          Show this help\n\n  Examples\n    $ maverick 006-build-subcommand --verbose\n    $ maverick -b 006-build-subcommand\n    $ maverick 006-build-subcommand -t custom-tasks.md\n`,
    {
      importMeta: import.meta,
      flags: {
        branch: { type: 'string', shortFlag: 'b' },
        tasks: { type: 'string', shortFlag: 't' },
        verbose: { type: 'boolean', shortFlag: 'v', default: false }
      }
    }
  )

  // Determine branch (positional overrides default, or --branch flag)
  let branch = cli.flags.branch || cli.input[0] || 'build-parity-opencode'
  // Derive project root (script lives in .maverick/src)
  const projectRoot = path.resolve(__dirname, '..', '..')
  // Compute default tasks path from branch unless overridden
  const defaultTasksPath = path.join(projectRoot, 'specs', branch, 'tasks.md')
  const tasksFile = cli.flags.tasks ? cli.flags.tasks : defaultTasksPath
  // If user provided a relative override, resolve relative to current working directory; otherwise keep absolute
  const resolvedTasksPath = path.isAbsolute(tasksFile) ? tasksFile : path.resolve(process.cwd(), tasksFile)

  const tasks = new Listr([
    {
      title: `Parse tasks file (${resolvedTasksPath === defaultTasksPath ? 'default' : 'override'}: ${resolvedTasksPath})`,
      task: async (ctx, task) => {
        ctx.phases = await parsePhases(resolvedTasksPath, process.cwd())
        if (cli.flags.verbose) {
          const summary = ctx.phases
            .map(p => `Phase ${p.identifier}: ${p.title} — ${p.outstandingTasks}/${p.totalTasks} outstanding`)
            .join('\n')
          task.output = summary
        } else {
          task.output = 'Tasks parsed'
        }
      },
      options: { persistentOutput: true }
    },
    {
      title: `Run workflow on branch ${branch}`,
      task: (ctx, task) => {
        const verbose = cli.flags.verbose
        const verboseLogger = m => { if (verbose) { task.output = m } }
        return runWorkflow({ branch, phases: ctx.phases, tasksPath: resolvedTasksPath, verbose, logger: verboseLogger })
      },
      options: { persistentOutput: true }
    }
  ], { renderer: 'verbose', rendererOptions: { showSubtasks: true, collapseErrors: false } })

  await tasks.run({})
}

main().catch(err => {
  console.error('Workflow failed:', err)
  process.exit(1)
})