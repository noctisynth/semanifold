#!/usr/bin/env node
'use strict';

const { runCli } = require('../index.js');

process.exitCode = runCli(process.argv.slice(2));
