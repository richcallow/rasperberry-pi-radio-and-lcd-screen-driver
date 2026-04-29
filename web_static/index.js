htmx.onLoad(function () {
    document.body.addEventListener("htmx:sseBeforeMessage", function (event) {
        if (event.target.querySelector("input[type=range][data-dragging=true]") !== null) {
            event.preventDefault();
        }
    });

    document.getElementById("toggleShowMenuButton").addEventListener("click", function () {
        document.getElementById("linksToMenu").classList.toggle("linksToMenuHidden");
    });
});