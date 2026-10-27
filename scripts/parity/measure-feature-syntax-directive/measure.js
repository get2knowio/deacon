// Measures the reference's dockerfilePrefixContent decision over a table.
// argv: semver module dir, oracle.js (from build_oracle.py), output fixture path
const fs = require('fs');

const o = require(process.argv[3]);
const out = process.argv[4];

// The reference's LN / vN version probes: versionMatch from raw output.
function versionMatch(raw) {
  if (raw === null) return undefined; // the probe threw
  const m = raw.match(/(?<major>[0-9]+)\.(?<minor>[0-9]+)\.(?<patch>[0-9]+)/);
  return m ? { versionString: raw, versionMatch: m[0] } : { versionString: raw };
}

const dockerfiles = {
  none: null,
  plain: 'FROM debian\n',
  'syntax-1': '# syntax=docker/dockerfile:1\nFROM debian\n',
  'syntax-1.2': '# syntax=docker/dockerfile:1.2\nFROM debian\n',
  'syntax-1.3.1': '# syntax=docker/dockerfile:1.3.1\nFROM debian\n',
  'syntax-1.4': '# syntax=docker/dockerfile:1.4\nFROM debian\n',
  'syntax-1.5-labs': '# syntax=docker/dockerfile:1.5-labs\nFROM debian\n',
  'syntax-untagged': '# syntax=docker/dockerfile\nFROM debian\n',
  'syntax-labs': '# syntax=docker/dockerfile:labs\nFROM debian\n',
  'syntax-docker-io': '# syntax=docker.io/docker/dockerfile:1.2\nFROM debian\n',
  'syntax-foreign': '# syntax=mycompany/frontend:2\nFROM debian\n',
  'syntax-spaced-crlf': '  #  syntax = docker/dockerfile:1.2\r\nFROM debian\r\n',
  'syntax-after-escape': '# escape=`\n# syntax=docker/dockerfile:1.2\nFROM debian\n',
  'syntax-after-blank': '\n# syntax=docker/dockerfile:1.2\nFROM debian\n',
  'syntax-uppercase-name': '# SYNTAX=docker/dockerfile:1.2\nFROM debian\n',
  'syntax-only-line': '# syntax=docker/dockerfile:1.2',
};

const engines = {
  'engine-29.7.2': '29.7.2',
  'engine-23.0.0': '23.0.0',
  'engine-22.99.99': '22.99.99',
  'engine-20.10.24': '20.10.24',
  'engine-podman-4.9.3': '4.9.3',
  'engine-two-part': '24.0',
  'engine-garbage': 'Cannot connect to the Docker daemon',
  'engine-failed': null,
};
const buildkits = {
  'buildkit-0.12.1': 'github.com/docker/buildx v0.12.1 30feaa1\n',
  'buildkit-0.8.0': 'github.com/docker/buildx v0.8.0 abc\n',
  'buildkit-0.7.9': 'github.com/docker/buildx v0.7.9 abc\n',
  'buildkit-buildah': 'buildah 1.33.7\n',
  'buildkit-failed': null,
};

const cases = [];
for (const [dfId, df] of Object.entries(dockerfiles)) {
  for (const [eId, e] of Object.entries(engines)) {
    for (const [bId, b] of Object.entries(buildkits)) {
      for (const omit of [false, true]) {
        const ev = versionMatch(e);
        const bv = versionMatch(b);
        // containerFeatures.ts: useBuildKitBuildContexts / skipDefaultSyntax
        const a = bv && bv.versionMatch ? o.nr(bv.versionMatch) : undefined;
        const Q = a ? !o.or(a, [0, 8, 0]) : false;
        const u = ev && ev.versionMatch ? o.nr(ev.versionMatch) : undefined;
        const l = u ? !o.or(u, [23, 0, 0]) : false;
        let parsed;
        if (df !== null) {
          parsed = o.EG(df);
          // Tj: --omit-syntax-directive deletes the parsed directive.
          if (parsed.preamble.directives.syntax && omit) delete parsed.preamble.directives.syntax;
        }
        const directive = o.prefix({ omitSyntaxDirective: omit }, l, Q, { dockerfile: parsed });
        cases.push([dfId, eId, bId, omit, directive]);
      }
    }
  }
}
const dfOut = {};
for (const [k, v] of Object.entries(dockerfiles)) {
  if (v === null) { dfOut[k] = null; continue; }
  const m = v.match(o.STRIP);
  dfOut[k] = { content: v, stripped: m ? v.slice(m[0].length) : v };
}
const body = [
  '{',
  ' "dockerfiles": ' + JSON.stringify(dfOut) + ',',
  ' "engineOutputs": ' + JSON.stringify(engines) + ',',
  ' "buildkitOutputs": ' + JSON.stringify(buildkits) + ',',
  ' "cases": [',
  cases.map(c => '  ' + JSON.stringify(c)).join(',\n'),
  ' ]',
  '}',
].join('\n');
fs.writeFileSync(out, body + '\n');
console.log(cases.length, 'cases;', cases.filter(c => c[4] === '# syntax=docker/dockerfile:1.4').length, 'inject');
