import { exec } from 'child_process';
import { platform } from 'os';

function decodeBase16(base16String: string): string {
    // 移除 '<<data HTML' 和 '>>' 
    const cleanHex = base16String
        .replace(/^<<\s*data HTML/i, '')
        .replace(/>>$/i, '')
        .trim();

    // 将每两个字符转换为一个字节，然后解码为 UTF-8 字符串
    const bytes = new Uint8Array(
        cleanHex.match(/.{1,2}/g)?.map(byte => parseInt(byte, 16)) || []
    );
    
    return new TextDecoder('utf-8').decode(bytes);
}

export async function getClipboardHtml(): Promise<string> {
    const currentPlatform = platform();
    
    return new Promise((resolve, reject) => {
        switch (currentPlatform) {
            case 'darwin': // macOS
                exec(`osascript -e 'the clipboard as «class HTML»'`, (error, stdout) => {
                    if (error) {
                        reject(error);
                        return;
                    }
                    try {
                        resolve(decodeBase16(stdout));
                    } catch (decodeError: any) {
                        reject(new Error(`Failed to decode clipboard content: ${decodeError.message}`));
                    }
                });
                break;
                
            case 'win32': // Windows
                // 使用 PowerShell 脚本获取剪贴板 HTML
                const psScript = `
                    Add-Type -AssemblyName System.Windows.Forms
                    if ([Windows.Forms.Clipboard]::ContainsData([Windows.Forms.DataFormats]::Html)) {
                        [Windows.Forms.Clipboard]::GetData([Windows.Forms.DataFormats]::Html)
                    }
                `;
                exec(`powershell -command "${psScript}"`, (error, stdout) => {
                    if (error) {
                        reject(error);
                        return;
                    }
                    resolve(stdout);
                });
                break;
                
            case 'linux':
                // 使用 xclip 获取剪贴板 HTML
                exec('xclip -selection clipboard -t text/html -o', (error, stdout) => {
                    if (error) {
                        reject(error);
                        return;
                    }
                    resolve(stdout);
                });
                break;
                
            default:
                reject(new Error(`Unsupported platform: ${currentPlatform}`));
        }
    });
}
