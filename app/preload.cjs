const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
    windowControls: (command) => ipcRenderer.send('window-controls', command)
});
