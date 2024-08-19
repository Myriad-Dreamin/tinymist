import { ProgressLocation, window, workspace } from "vscode";
import { tinymist } from "./lsp";
import * as fs from 'fs';
import simpleGit from 'simple-git';

// error message
const repositoryErrorMessage = 'Can not find remoteUrl, please make sure you have configured "tinymist.localPackage.remoteUrl" in settings.';
const dataDirErrorMessage = 'Can not find package directory.';

// main branch
const mainBranch = 'main';

export function getRemoteRepo() {
  if (workspace.getConfiguration().has('tinymist.localPackage.remoteUrl')) {
    return workspace.getConfiguration().get('tinymist.localPackage.remoteUrl') as string;
  }
  return '';
}

export async function getRemoteRepoGit() {
  // 1. get remote repository
  const remoteUrl = getRemoteRepo();
  if (!remoteUrl) {
    window.showErrorMessage(repositoryErrorMessage);
    return;
  }
  // 2. if typstDir not exist, create it
  const typstPackageDir = await tinymist.getResource('/dirs/local-packages');
  if (!typstPackageDir) {
    window.showErrorMessage(dataDirErrorMessage);
    return;
  }
  const typstDir = await fs.promises.realpath(`${typstPackageDir}/..`);
  try {
    await fs.promises.access(typstDir);
  } catch (err) {
    await fs.promises.mkdir(typstDir, { recursive: true });
  }
  // 3. if .git not exist, init it or clone it
  const git = simpleGit(typstDir);
  if (!(await git.checkIsRepo())) {
    if ((await fs.promises.readdir(typstDir)).length === 0) {
      await git.clone(remoteUrl, typstDir);
    } else {
      await git.init({ '--initial-branch': mainBranch });
      await git.add('.');
      await git.commit('init');
    }
  }
  // 4. add remote, if remote exist and is not the same, ask
  const originRemotes = (await git.getRemotes(true)).filter(remote => remote.name === 'origin');
  const originRemote = originRemotes.length > 0 ? originRemotes[0] : null;
  if (originRemote && originRemote.refs.fetch !== remoteUrl) {
    const answer = await window.showQuickPick(['Yes', 'No'], {
      placeHolder: 'Remote origin already exists, do you want to replace it?'
    });
    if (answer === 'Yes') {
      await git.removeRemote('origin');
      await git.addRemote('origin', remoteUrl);
    }
  }
  if (!originRemote) {
    await git.addRemote('origin', remoteUrl);
  }
  // 5. return git
  return git;
}

export async function commandPushRemoteRepo() {
  await window.withProgress({
    location: ProgressLocation.Notification,
    title: 'Syncing',
    cancellable: true
  }, async (progress, token) => {
    // 0. cancel callback
    token.onCancellationRequested(() => {
      window.showInformationMessage('Syncing with remote repository canceled.');
    });
    // 1. get remote repository
    progress.report({ increment: 15, message: 'Getting remote repository...' });
    if (token.isCancellationRequested) { return; }
    const git = await getRemoteRepoGit();
    if (!git) {
      return;
    }
    // 2. git add all files to stage
    progress.report({ increment: 15, message: 'Adding files to stage...' });
    await git.add('.');
    if (token.isCancellationRequested) { return; }
    // 3. git commit with timestamp, if there are changes
    progress.report({ increment: 15, message: 'Committing...' });
    const status = await git.status();
    if (status.files.length !== 0) {
      const timestamp = new Date().toISOString();
      await git.commit(timestamp);
    }
    if (token.isCancellationRequested) { return; }
    // 4. git pull and keep all local changes (merge strategy)
    progress.report({ increment: 15, message: 'Pulling...' });
    try {
      await git.pull('origin', mainBranch);
    } catch (err) {
      // ignore
    }
    if (token.isCancellationRequested) { return; }
    // 5. git push
    progress.report({ increment: 15, message: 'Pushing...' });
    await git.push(['origin', mainBranch]);
    if (token.isCancellationRequested) { return; }
  });
}

export async function commandPullRemoteRepo() {
  await window.withProgress({
    location: ProgressLocation.Notification,
    title: 'Pulling',
    cancellable: true
  }, async (progress, token) => {
    // 0. cancel callback
    token.onCancellationRequested(() => {
      window.showInformationMessage('Pulling from remote repository canceled.');
    });
    // 1. get remote repository git
    progress.report({ increment: 33, message: 'Getting remote repository...' });
    const git = await getRemoteRepoGit();
    if (!git) {
      return;
    }
    if (token.isCancellationRequested) { return; }
    // 2. git pull and keep all remote changes (merge strategy)
    progress.report({ increment: 33, message: 'Pulling...' });
    await git.pull('origin', mainBranch);
    if (token.isCancellationRequested) { return; }
  });
}