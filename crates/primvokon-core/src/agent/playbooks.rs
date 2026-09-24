//! Built-in playbooks: prompt sections for recurring tasks. Users can edit them in settings.

use crate::settings::Playbook;

pub fn defaults() -> Vec<Playbook> {
    vec![
        Playbook {
            id: "teams-reply".into(),
            name: "Reply in Microsoft Teams".into(),
            prompt: "The host runs Microsoft Teams. Read the open chat with read_screen first. \
Identify the latest message that needs an answer and who wrote it. Draft a reply in the same language and \
tone as the conversation, short and friendly, matching how the user usually writes. Click into the message \
box, type the reply with type_text, then press Enter with send_shortcut. Never send anything before the \
reply text was approved in ask mode."
                .into(),
        },
        Playbook {
            id: "outlook-reply".into(),
            name: "Reply in Outlook".into(),
            prompt: "The host runs Microsoft Outlook. Read the selected e-mail with read_screen. \
Open the reply (shortcut ControlLeft,KeyR), wait for the compose window, then type a well-structured reply: \
greeting, answer to every question asked, closing. Use the language of the original mail. Send with \
ControlLeft,Enter only after the draft was approved."
                .into(),
        },
        Playbook {
            id: "confluence-doc".into(),
            name: "Document in Confluence".into(),
            prompt: "The host has Confluence open in a browser. Read the current page with read_screen. \
Create or edit the page as requested: use headings, bullet lists and tables where helpful, keep existing \
content, and describe what you changed at the end. Publish only after approval."
                .into(),
        },
        Playbook {
            id: "general".into(),
            name: "General assistant".into(),
            prompt: "Complete the task the user gives you on the host machine. Read the screen before \
and after every action to verify the effect. Prefer keyboard shortcuts over mouse clicks."
                .into(),
        },
    ]
}
