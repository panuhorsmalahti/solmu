const input = document.querySelector('#guide-filter')
const groups = [...document.querySelectorAll('.guide-group')]
input.addEventListener('input', () => {
  const query = input.value.trim().toLocaleLowerCase()
  let matches = 0
  for (const group of groups) {
    let visible = 0
    for (const item of group.querySelectorAll('li')) {
      item.hidden = !item.textContent.toLocaleLowerCase().includes(query)
      if (!item.hidden) visible++
    }
    group.hidden = visible === 0
    matches += visible
  }
  document.querySelector('#no-guides').hidden = matches > 0
})

const menu = document.querySelector('.guide-menu')
const desktop = matchMedia('(min-width: 901px)')
const resizeMenu = () => { menu.open = desktop.matches }
resizeMenu()
desktop.addEventListener('change', resizeMenu)
