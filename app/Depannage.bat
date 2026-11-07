@echo off
rem Lance Rift Launcher avec une fenetre qui affiche les erreurs eventuelles.
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -NoExit -File "%~dp0RiftLauncher.ps1"
