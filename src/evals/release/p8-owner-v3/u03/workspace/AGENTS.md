# Implementation guidance

Use CommonJS and built-in Node modules only. src/domain/window.cjs owns numeric validation and page-window policy; src/api/page.cjs owns array/options validation and response construction and must call the domain function. Use TypeError('invalid pagination') for invalid input. Do not add dependencies, execute install commands or change unrelated files. Run node --test test/page.test.cjs after integrating source changes.
