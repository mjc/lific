const {inspect} = require('node:util');

// Optional JSONL evidence alongside the human-readable Node test report.
module.exports = async function* report(source) {
  for await (const event of source) {
    if (!['test:pass', 'test:fail', 'test:summary'].includes(event.type)) continue;
    const {name, file, line, column, nesting, details, ...summary} = event.data;
    yield `${JSON.stringify({event: event.type, name, file, line, column, nesting,
      ...(details ? {duration_ms: details.duration_ms, error: details.error ? inspect(details.error, {depth: 5}) : undefined} : summary)})}\n`;
  }
};
