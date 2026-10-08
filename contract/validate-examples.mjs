// Validates every file in contract/examples/ against its schema in contract/openapi.yaml,
// and fails when an operation has no example at all.
//
// The second half is the point. A validator that only checks the files present will pass
// a contract with no examples whatsoever, which is exactly the state this contract is
// meant to replace. So the operation list is read out of the OpenAPI document and each
// operation is required to have its files, by name:
//
//   examples/<operationId>.response.json   the success response (2xx)
//   examples/<operationId>.request.json    the request body, when the operation takes one
//   examples/<operationId>.request.<v>.json   further forms of that body, optional
//   examples/<operationId>.response.<v>.json  further forms of that response, optional
//   examples/errors/<name>.json            any number, each checked against Error
//
// The variant forms exist because one example cannot show an optional field both present
// and absent, and both forms need checking. They are optional and additional: the
// canonical file is still required, so a variant cannot be used to dodge the rule above.
//
// Exit 0 when everything validates and nothing is missing, 1 otherwise.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import YAML from 'yaml';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const here = path.dirname(fileURLToPath(import.meta.url));
const specPath = path.join(here, 'openapi.yaml');
const examplesDir = path.join(here, 'examples');
const errorsDir = path.join(examplesDir, 'errors');

const spec = YAML.parse(fs.readFileSync(specPath, 'utf8'));

const ajv = new Ajv2020({ strict: false, allErrors: true });
addFormats(ajv);
ajv.addSchema(spec, 'openapi.yaml');

const problems = [];
const checked = [];

/** Resolve a local $ref inside the document, one hop at a time. */
function deref(node, seen = 0) {
  if (!node || typeof node !== 'object' || !node.$ref) return node;
  if (seen > 10) throw new Error(`$ref nested more than 10 deep at ${node.$ref}`);
  if (!node.$ref.startsWith('#/')) throw new Error(`external $ref not supported: ${node.$ref}`);
  let target = spec;
  for (const part of node.$ref.slice(2).split('/')) {
    target = target?.[part.replace(/~1/g, '/').replace(/~0/g, '~')];
  }
  if (target === undefined) throw new Error(`$ref does not resolve: ${node.$ref}`);
  return deref(target, seen + 1);
}

const escape = (segment) => String(segment).replace(/~/g, '~0').replace(/\//g, '~1');

/**
 * A reference to the schema *inside the registered document*, never a copy of it.
 *
 * Copying the schema out and compiling it on its own looks equivalent and is not: the
 * schema's own `$ref: '#/components/schemas/...'` would then resolve against the copy,
 * whose root has no `components`, and every nested reference fails to resolve. Pointing
 * into `openapi.yaml` keeps the base URI right, so nested references resolve as written.
 */
function schemaRefFor(pointerToContainer, container) {
  if (!container?.content?.['application/json']?.schema) return null;
  return `openapi.yaml#${pointerToContainer}/content/application~1json/schema`;
}

function validateFile(file, schemaRef, label) {
  let data;
  try {
    data = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    problems.push(`${path.relative(here, file)}: not valid JSON — ${error.message}`);
    return;
  }
  let validate;
  try {
    validate = ajv.getSchema(schemaRef);
    if (!validate) {
      problems.push(`${label}: schema ${schemaRef} not found in the document`);
      return;
    }
  } catch (error) {
    problems.push(`${label}: schema would not compile — ${error.message}`);
    return;
  }
  if (validate(data)) {
    checked.push(`${path.relative(here, file)}  ->  ${label}`);
  } else {
    for (const error of validate.errors) {
      const where = error.instancePath || '(root)';
      problems.push(
        `${path.relative(here, file)}: ${where} ${error.message}` +
          (error.params && Object.keys(error.params).length
            ? ` ${JSON.stringify(error.params)}`
            : '')
      );
    }
  }
}

/**
 * The optional extra forms of one operation's example — `<id>.request.<variant>.json`.
 * Returns full paths, sorted, so the run order does not depend on the directory's.
 */
function variantsOf(id, direction) {
  const pattern = new RegExp(`^${id}\\.${direction}\\.[A-Za-z0-9-]+\\.json$`);
  return fs
    .readdirSync(examplesDir)
    .filter((name) => pattern.test(name))
    .sort()
    .map((name) => path.join(examplesDir, name));
}

const METHODS = ['get', 'put', 'post', 'patch', 'delete', 'head', 'options', 'trace'];
const seenIds = new Set();
const expected = new Set();
let operationCount = 0;

for (const [route, item] of Object.entries(spec.paths ?? {})) {
  for (const method of METHODS) {
    const operation = item[method];
    if (!operation) continue;
    operationCount += 1;

    const id = operation.operationId;
    if (!id) {
      problems.push(`${method.toUpperCase()} ${route}: no operationId, so it can have no example`);
      continue;
    }
    if (seenIds.has(id)) {
      problems.push(`operationId ${id} is used more than once`);
      continue;
    }
    seenIds.add(id);

    // The success response.
    const successCode = Object.keys(operation.responses ?? {}).find((code) =>
      /^2\d\d$/.test(code)
    );
    if (!successCode) {
      problems.push(`${id}: no 2xx response declared`);
    } else {
      const responseFile = path.join(examplesDir, `${id}.response.json`);
      expected.add(path.basename(responseFile));
      const responsePointer = `/paths/${escape(route)}/${method}/responses/${escape(successCode)}`;
      const pointer = schemaRefFor(responsePointer, deref(operation.responses[successCode]));
      if (!pointer) {
        problems.push(`${id}: the ${successCode} response declares no application/json schema`);
      } else if (!fs.existsSync(responseFile)) {
        problems.push(`${id}: missing example ${path.relative(here, responseFile)}`);
      } else {
        validateFile(responseFile, pointer, `${id} ${successCode} response`);
        for (const variant of variantsOf(id, 'response')) {
          expected.add(path.basename(variant));
          validateFile(variant, pointer, `${id} ${successCode} response (variant)`);
        }
      }
    }

    // The request body, when there is one.
    if (operation.requestBody) {
      const requestFile = path.join(examplesDir, `${id}.request.json`);
      expected.add(path.basename(requestFile));
      const bodyPointer = `/paths/${escape(route)}/${method}/requestBody`;
      const pointer = schemaRefFor(bodyPointer, deref(operation.requestBody));
      if (!pointer) {
        problems.push(`${id}: the request body declares no application/json schema`);
      } else if (!fs.existsSync(requestFile)) {
        problems.push(`${id}: missing example ${path.relative(here, requestFile)}`);
      } else {
        validateFile(requestFile, pointer, `${id} request body`);
        for (const variant of variantsOf(id, 'request')) {
          expected.add(path.basename(variant));
          validateFile(variant, pointer, `${id} request body (variant)`);
        }
      }
    }

    // Every operation must be able to fail in the one shape.
    const hasErrorResponse = Object.keys(operation.responses ?? {}).some((code) =>
      /^[45]\d\d$/.test(code)
    );
    if (!hasErrorResponse) {
      problems.push(`${id}: declares no 4xx or 5xx response, so its error shape is unstated`);
    }
  }
}

// Error examples, all against the one Error schema.
if (fs.existsSync(errorsDir)) {
  for (const name of fs.readdirSync(errorsDir).sort()) {
    if (!name.endsWith('.json')) continue;
    validateFile(
      path.join(errorsDir, name),
      'openapi.yaml#/components/schemas/Error',
      'Error'
    );
  }
}

// A file nobody claims is a file nobody maintains.
for (const name of fs.readdirSync(examplesDir).sort()) {
  if (!name.endsWith('.json')) continue;
  if (!expected.has(name)) {
    problems.push(
      `examples/${name}: no operation claims this file; name it <operationId>.response.json, ` +
        `<operationId>.request.json, either with a .<variant> before .json, ` +
        `or move it under examples/errors/`
    );
  }
}

for (const line of checked) console.log(`  ok  ${line}`);
console.log(
  `\n${operationCount} operations, ${checked.length} example files checked against their schemas.`
);

if (problems.length) {
  console.error(`\n${problems.length} problem(s):\n`);
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}

console.log('Every operation has its examples and every example matches its schema.');
