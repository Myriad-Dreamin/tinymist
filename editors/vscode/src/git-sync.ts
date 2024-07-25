import { ProgressLocation, window, workspace } from "vscode";
import { dataDirErrorMessage, getTypstDir } from "./package-manager";
import * as fs from 'fs';
import simpleGit from 'simple-git';

// error message
const syncRepoErrorMessage = 'Can not find Sync Repo, please make sure you have configured Sync Repo in settings.';

// main branch
const mainBranch = 'main';

export function getSyncRepo() {
  if (workspace.getConfiguration().has('tinymist.syncRepo')) {
    return workspace.getConfiguration().get('tinymist.syncRepo') as string;
  }
  return '';
}

export async function getSyncRepoGit() {
  // 1. get syncRepo
  const syncRepo = getSyncRepo();
  if (!syncRepo) {
    window.showErrorMessage(syncRepoErrorMessage);
    return;
  }
  // 2. if typstDir not exist, create it
  const typstDir = getTypstDir();
  if (!typstDir) {
    window.showErrorMessage(dataDirErrorMessage);
    return;
  }
  try {
    await fs.promises.access(typstDir);
  } catch (err) {
    await fs.promises.mkdir(typstDir, { recursive: true });
  }
  // 3. if .git not exist, init it or clone it
  const git = simpleGit(typstDir);
  if (!(await git.checkIsRepo())) {
    if ((await fs.promises.readdir(typstDir)).length === 0) {
      await git.clone(syncRepo, typstDir);
    } else {
      await git.init({ '--initial-branch': mainBranch });
      await git.add('.');
      await git.commit('init');
    }
  }
  // 4. add remote, if remote exist and is not the same, ask
  const originRemotes = (await git.getRemotes(true)).filter(remote => remote.name === 'origin');
  const originRemote = originRemotes.length > 0 ? originRemotes[0] : null;
  if (originRemote && originRemote.refs.fetch !== syncRepo) {
    const answer = await window.showQuickPick(['Yes', 'No'], {
      placeHolder: 'Remote origin already exists, do you want to replace it?'
    });
    if (answer === 'Yes') {
      await git.removeRemote('origin');
      await git.addRemote('origin', syncRepo);
    }
  }
  if (!originRemote) {
    await git.addRemote('origin', syncRepo);
  }
  // 5. return git
  return git;
}

export async function commandPushRepo() {
  await window.withProgress({
    location: ProgressLocation.Notification,
    title: 'Syncing',
    cancellable: true
  }, async (progress, token) => {
    // 0. cancel callback
    token.onCancellationRequested(() => {
      window.showInformationMessage('Syncing with syncRepo canceled.');
    });
    // 1. get syncRepo git
    progress.report({ increment: 15, message: 'Getting syncRepo...' });
    if (token.isCancellationRequested) { return; }
    const git = await getSyncRepoGit();
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

export async function commandPullRepo() {
  await window.withProgress({
    location: ProgressLocation.Notification,
    title: 'Pulling',
    cancellable: true
  }, async (progress, token) => {
    // 0. cancel callback
    token.onCancellationRequested(() => {
      window.showInformationMessage('Pulling from syncRepo canceled.');
    });
    // 1. get syncRepo git
    progress.report({ increment: 33, message: 'Getting syncRepo...' });
    const git = await getSyncRepoGit();
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