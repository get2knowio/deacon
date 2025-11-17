#!/usr/bin/env node
/**
 * Sample Node.js application demonstrating exec workspace discovery
 */

console.log('=== Node.js Application ===');
console.log(`Node version: ${process.version}`);
console.log(`Platform: ${process.platform}`);
console.log(`Architecture: ${process.arch}`);
console.log(`Current user: ${process.env.USER || 'unknown'}`);
console.log(`Working directory: ${process.cwd()}`);
console.log(`NODE_ENV: ${process.env.NODE_ENV || 'not-set'}`);
console.log(`WORKSPACE_NAME: ${process.env.WORKSPACE_NAME || 'not-set'}`);
console.log('=== Execution Complete ===');
