app [main] { pf: platform "platform.roc" }

main = {
    default: Job {
        command: Command {
            tool: SystemTool { name: "test" },
            args: [],
        },
        job: [],
        inputFiles: ["foo"],
    },
}

