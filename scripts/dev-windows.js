const { spawn, execSync } = require('child_process');

// Get ports from setup script
const frontendPortOutput = execSync('node scripts/setup-dev-environment.js frontend', { encoding: 'utf-8' }).trim();
const backendPortOutput = execSync('node scripts/setup-dev-environment.js backend', { encoding: 'utf-8' }).trim();

// Parse JSON output
const frontendPort = JSON.parse(frontendPortOutput);
const backendPort = JSON.parse(backendPortOutput);

// Set environment variables
process.env.FRONTEND_PORT = frontendPort.toString();
process.env.BACKEND_PORT = backendPort.toString();
process.env.VK_ALLOWED_ORIGINS = `http://localhost:${frontendPort}`;
process.env.VITE_VK_SHARED_API_BASE = process.env.VK_SHARED_API_BASE || '';
process.env.VITE_OPEN = 'false';
process.env.DISABLE_WORKTREE_CLEANUP = '1';
process.env.RUST_LOG = 'debug';

console.log(`Starting development servers...`);
console.log(`Frontend port: ${frontendPort}`);
console.log(`Backend port: ${backendPort}`);

// Start backend
const backend = spawn('cargo watch -w crates -x "run --bin server"', {
  stdio: 'inherit',
  shell: true,
  env: process.env
});

// Start frontend
const frontend = spawn('pnpm', ['run', 'frontend:dev'], {
  stdio: 'inherit',
  shell: true,
  env: process.env
});

// Handle process termination
const cleanup = () => {
  backend.kill();
  frontend.kill();
  process.exit();
};

process.on('SIGINT', cleanup);
process.on('SIGTERM', cleanup);

backend.on('exit', (code) => {
  console.log(`Backend exited with code ${code}`);
  frontend.kill();
  process.exit(code);
});

frontend.on('exit', (code) => {
  console.log(`Frontend exited with code ${code}`);
  backend.kill();
  process.exit(code);
});
