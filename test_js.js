const fs = require('fs');
const html = fs.readFileSync('c:/Users/RyanClanton/marksmen/crates/marksmen-editor/src/index.html', 'utf8');
const code = fs.readFileSync('c:/Users/RyanClanton/marksmen/crates/marksmen-editor/src/main.js', 'utf8');

const jsdom = require('jsdom');
const { JSDOM } = jsdom;
const dom = new JSDOM(html, { url: "http://localhost", runScripts: "outside-only" });

global.window = dom.window;
global.document = dom.window.document;
global.navigator = dom.window.navigator;
global.Event = dom.window.Event;
global.MutationObserver = dom.window.MutationObserver;
global.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
global.requestAnimationFrame = (cb) => cb();
global.localStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {} };

try {
    eval(code);
    console.log("No global errors!");
} catch (e) {
    console.error("ERROR CAUGHT:");
    console.error(e);
}
