import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export type WindowControlCommand = 'min' | 'max' | 'close';

export async function controlDesktopWindow(command: WindowControlCommand) {
  if (!isTauri()) {
    return;
  }

  const appWindow = getCurrentWindow();

  try {
    switch (command) {
      case 'min':
        await appWindow.minimize();
        break;
      case 'max':
        await appWindow.toggleMaximize();
        break;
      case 'close':
        await appWindow.close();
        break;
    }
  } catch (error) {
    console.warn(`Window control "${command}" failed.`, error);
  }
}
