# Slice the reference CLI's compiled bundle into oracle.js for measure.js (#732).
# argv: <cli>/dist/spec-node/devContainersSpecCLI.js <out oracle.js>
import sys
s = open(sys.argv[1]).read()
out = sys.argv[2]


def between(a, b, start=0):
    i = s.index(a, start)
    j = s.index(b, i + len(a))
    return s[i:j]


parts = []
parts.append(between("function nr(", "async function Pe("))
parts.append(between("function Nj(", "function QG("))
parts.append(between("function bj(", "function Gj("))
parts.append(between("function lG(", "function hG("))


def lit(name, end):
    i = s.index(name + "=/")
    j = s.index(end, i)
    return s[i:j + len(end) - 1]


parts.append("var " + lit("Sj", "/mi,") + ";")
parts.append("var " + lit("Rj", "/gmi,") + ";")
parts.append("var " + lit("kj", "/,Mj=") + ";")
k = s.index("dockerfile:1.4")
a = s.rindex("N=`${o.omitSyntaxDirective", 0, k)
b = s.index("\nARG _DEV_CONTAINERS_BASE_IMAGE", k)
expr = s[a + 2:b] + "`"
parts.append(
    "function prefix(o,l,Q,r){var J;let X=(J=r.dockerfile)==null?void 0:J.preamble.directives.syntax;return "
    + expr + ";}")
# the callers' strip regex, verbatim
c = s.index("v=a.match(/^\\s*#\\s*syntax\\s*=.*[\\r\\n]/g)")
strip = s[c + len("v=a.match("):s.index(")", c + len("v=a.match(") + 5)]
parts.append("const STRIP=" + strip + ";")
open(out, "w").write("const BG=require(process.argv[2]);\n" + "\n".join(parts)
                     + "\nmodule.exports={nr,or,EG,lG,prefix,STRIP};\n")
print("ok", expr, strip)
