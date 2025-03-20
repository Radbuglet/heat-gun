let EXPERIMENT = 0;

(async () => {
    async function makeRedImage() {
        const canvas = document.createElement("canvas");
        canvas.width = 100;
        canvas.height = 100;

        const ctx = canvas.getContext("2d");

        ctx.fillStyle = "red";
        ctx.fillRect(0, 0, 100, 100);

        return await createImageBitmap(canvas);
    }

    const red_image = await makeRedImage();

    const canvas = document.getElementById("canvas");
    const ctx = canvas.getContext("2d");

    const fps = document.getElementById("fps");
    let frame_counter = 0;
    let last_second = performance.now();

    function tick() {
        requestAnimationFrame(tick);

        const now = performance.now();

        if (now - last_second > 1000) {
            last_second = now;
            fps.innerText = frame_counter;
            frame_counter = 0;
        }

        frame_counter += 1;

        ctx.clearRect(0, 0, canvas.width, canvas.height);
        ctx.save();

        switch (EXPERIMENT) {
        case 0:
            ctx.globalAlpha = 0.1;
            ctx.fillStyle = "blue";

            for (let i = 0; i < 10_000; i++) {
                ctx.fillRect(100, 100, 100, 100);
                ctx.drawImage(red_image, 100, 100);
            }

            break;
        case 1:
            ctx.globalAlpha = 0.1;
            ctx.fillStyle = "blue";

            for (let i = 0; i < 20_000; i++) {
                ctx.drawImage(red_image, 100, 100);
            }

            break;
        case 2:
            ctx.globalAlpha = 0.1;
            ctx.fillStyle = "blue";

            for (let i = 0; i < 20_000; i++) {
                ctx.fillRect(100, 100, 100, 100);
            }

            break;
        case 3:
            ctx.fillStyle = "blue";

            for (let i = 0; i < 10_000; i++) {
                ctx.fillRect(100, 100, 100, 100);
                ctx.drawImage(red_image, 100, 100);
            }

            break;
        case 4:
            ctx.fillStyle = "blue";

            for (let i = 0; i < 10_000; i++) {
                ctx.fillRect(100, 100, 100, 100);
            }

            for (let i = 0; i < 10_000; i++) {
                ctx.drawImage(red_image, 100, 100);
            }

            break;
        case 5:
            ctx.globalAlpha = 0.1;
            ctx.fillStyle = "blue";

            for (let i = 0; i < 10_000; i++) {
                ctx.fillRect(100, 100, 100, 100);
            }

            for (let i = 0; i < 10_000; i++) {
                ctx.drawImage(red_image, 100, 100);
            }

            break;
        }

        ctx.restore();
    }

    requestAnimationFrame(tick);
})();
