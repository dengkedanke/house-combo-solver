// #18：ESLint flat config（本项目此前无任何 ESLint 配置，
// App.tsx 中的 eslint-disable 注释处于无效状态；引入后即生效）
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';

export default tseslint.config(
  {
    ignores: [
      'dist',
      'node_modules',
      'src-tauri/target',
      'src-tauri/gen',
      '*.config.js',
      'vite.config.ts',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
      // 前端无测试文件时保持整洁（冒烟脚本用 node 运行，ESLint 可忽略）
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
    },
  },
);
