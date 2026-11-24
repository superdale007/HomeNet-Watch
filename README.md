# HomeNet Watch

**Like a baby monitor, but for your network.**

HomeNet Watch keeps an eye on everything connected to your home network so you don't have to. It finds every device, draws you a map, and pings you when your smart TV wanders off or a mystery gadget joins the party. It also shows which programs on your computer reach out to the internet, where they're connecting, and who made them. If something you trusted suddenly changes, you'll know.

Got a Synology, QNAP, or UGREEN NAS? It'll watch your disks, temps, and storage too.

Everything stays on your computer. No accounts, no cloud, no nonsense. Just your network, explained in plain English.

## What it does

- **Your home network:** finds every device, checks that it's online and how fast it answers, draws a network map, and alerts you when something drops off, slows down, or shows up for the first time.
- **Your computer's internet use:** live and past upload/download, which programs connect where, countries on a world map, and who signed each program.
- **Your NAS:** storage space, disk health, temperatures, processor, and traffic over SNMP.
- **Extras:** plain-language insights, a speed test, weekly reports, and backup and restore.

## Download

Grab the file for your computer from the [Releases](../../releases) page.

| Your computer | File | How to start it |
|---|---|---|
| Windows 10 / 11 | `homenet-watch-windows-x86_64.exe` | Put it in a folder of your own (for example `Documents\HomeNet Watch`) and double-click it. |
| Mac (Intel or Apple silicon) | `homenet-watch-macos-universal` | In Terminal: `chmod +x homenet-watch-macos-universal`, then right-click the file and choose **Open**. |
| Linux (64-bit PC) | `homenet-watch-linux-x86_64` | `chmod +x homenet-watch-linux-x86_64`, then run it. Needs `ping` and `curl`. |

**Seeing "Windows protected your PC" or a Mac security message?** HomeNet Watch isn't code-signed yet. On Windows, click **More info** then **Run anyway**. On a Mac, right-click and choose **Open**, or go to **System Settings > Privacy & Security** and click **Open Anyway**. You only need to do this once.

**Tip:** keep it out of Program Files. In a folder of your own, it can update itself with one click.

## Privacy

Everything HomeNet Watch records stays on your computer. It only contacts the internet for:

- Your outside address (api.ipify.org, every 30 minutes)
- Speed tests, only when you press Start (speed.cloudflare.com)
- Country lookup for the world map, only if you turn it on (ip-api.com). Only internet addresses are sent, never your home devices or any content.
- Checks you set up yourself (web pages, DNS)
- Update checks (api.github.com), which you can turn off

## License

GPL-3.0. See [LICENSE](LICENSE).

## Support

HomeNet Watch is made by **DDID: [DaleDumbsITDown.com](https://daledumbsitdown.com)**. If it helps you out, you can buy me a coffee on [Ko-fi](https://ko-fi.com/superdale) or via PayPal (@dalemeredith).
